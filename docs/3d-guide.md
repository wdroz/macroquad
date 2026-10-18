# 3D collision guide

This fork adds 3D queries to `macroquad::math`. They are also in the prelude (`use macroquad::prelude::*`). The same types are in `macroquad::math` for a test or a tool that must not open a window.

Nothing here assumes an up axis. `Camera3D::up` stays caller-defined. The examples in this repository use **Y-up**, which matches a glTF file exported from Blender. Sizes are unitless. A glTF scene is in meters, and `get_frame_time` is in seconds, so a gravity of `(0, -9.8, 0)` is meters per second squared when the level is left in those units.

`draw_cube` / `draw_cube_wires` take a center and a **full** size. `Aabb::from_center_size` uses that same pair. `Obb::half_extents` is half of that full size. `Capsule` `a` and `b` are the inner segment, so the hull extends `radius` past each end.

There is no rigid-body world and no character controller in the library yet. `Contact`, `SweepHit`, and `slide_sphere` are queries a game uses to write those itself. The physics and character sections below are that application code. The narrative for loading a Blender level is [blender-to-macroquad.md](blender-to-macroquad.md).

## Ray hits

Every ray query returns `Option<RayHit>`:

- `t` is the parameter along `origin + direction * t`. It is a distance only when `direction` is a unit vector.
- `point` is the world position of the hit.
- `normal` is a unit vector, or zero. `t == 0` does not by itself mean the normal is zero.

Hits with `t < 0` are behind the origin and are rejected. Boundaries are inclusive, same as `Rect`: touching a face, edge, or corner counts. A non-finite origin, direction, or shape misses.

`Aabb`, `Obb`, `Sphere`, and `Capsule` have an interior:

- An origin inside the shape reports `t == 0` and a zero normal.
- An origin on the surface with `direction` pointing outward, or a zero direction, also reports `t == 0` and a zero normal.
- An origin on the surface with `direction` pointing inward or tangent reports `t == 0` and the outward normal.

`Plane` and `Triangle` have no interior. A hit reports the plane normal or the triangle's winding normal, including when the origin already lies on the shape. That normal is not flipped to face the ray.

```rust
use macroquad::math::{Aabb, vec3, Vec3};

let wall = Aabb::from_min_max(Vec3::ZERO, Vec3::ONE);
let hit = wall
    .ray_intersect(vec3(-1.0, 0.5, 0.5), Vec3::X)
    .expect("enters the -X face");
assert_eq!(hit.t, 1.0);
assert_eq!(hit.normal, -Vec3::X);
```

## AABB

`Aabb` stores `min` and `max` corners in world space. Constructors do not swap an inverted box. Call `normalized` before a query if a component of `min` is greater than `max`.

```rust
use macroquad::math::{Aabb, vec3};

let crate_ = Aabb::from_center_size(vec3(0.0, 0.5, 0.0), vec3(1.0, 1.0, 1.0));
let door = Aabb::from_min_max(vec3(0.4, 0.0, 0.0), vec3(0.6, 1.0, 0.2));

assert!(crate_.overlaps(&door));
let shared = crate_.intersect(door).expect("they touch");
assert!(shared.contains(vec3(0.5, 0.5, 0.1)));

let moved = crate_.offset(vec3(3.0, 0.0, 0.0));
assert!(!moved.overlaps(&door));
```

Useful methods: `from_points`, `center`, `size`, `half_extents`, `contains`, `combine_with`, `expanded`, `closest_point`, and `ray_intersect`. `expanded` does not re-normalize, so a large negative margin can invert the box.

## Sphere

`Sphere` is a center and a radius, the same pair `draw_sphere` takes. A negative or non-finite radius is empty: it contains nothing and overlaps nothing. A radius of zero is a point. `overlaps_aabb` uses the closest point on the box, so a sphere can rest on a corner.

```rust
use macroquad::math::{Aabb, Sphere, Vec3, vec3};

let ball = Sphere::new(vec3(0.0, 1.0, 0.0), 0.5);
let floor = Aabb::from_center_size(vec3(0.0, -0.05, 0.0), vec3(4.0, 0.1, 4.0));
assert!(!ball.overlaps_aabb(&floor));
assert!(ball.offset(vec3(0.0, -0.5, 0.0)).overlaps_aabb(&floor));
assert_eq!(ball.closest_point(Vec3::ZERO), vec3(0.0, 0.5, 0.0));
```

## Rays

`Ray` is an origin and a direction. The direction does not have to be a unit vector. `normalized` returns a copy with a unit direction (a zero direction becomes `Vec3::ZERO`). `point_at(t)` does not reject a negative `t`; the slice queries do.

`closest_aabb`, `closest_obb`, `closest_sphere`, `closest_plane`, `closest_triangle`, and `closest_capsule` scan a slice and return `Option<ClosestHit>`. `index` is the position in that slice. Ties keep the earlier index. Compare hits from different slices by `hit.t` only when both queries used the same direction.

```rust
use macroquad::math::{Aabb, Ray, Sphere, vec3, Vec3};

let ray = Ray::new(vec3(0.0, 0.5, 4.0), -Vec3::Z);
let boxes = [Aabb::from_center_size(vec3(0.0, 0.5, 0.0), Vec3::ONE)];
let spheres = [Sphere::new(vec3(0.0, 0.5, 2.0), 0.25)];

let box_hit = ray.closest_aabb(&boxes).expect("wall");
let sphere_hit = ray.closest_sphere(&spheres).expect("orb");
let first = if sphere_hit.hit.t < box_hit.hit.t {
    sphere_hit.hit
} else {
    box_hit.hit
};
assert!(first.t > 0.0);
```

`Camera3D::screen_to_ray` unprojects a screen point into a `Ray`. The point is in pixels of the surface the camera draws to, origin at the top left, y down, which is `mouse_position` when the camera draws to the window. A viewport is that camera's GL viewport, converted the same way as `Camera2D::world_to_screen`. A render target uses the texture's size instead of the window. The direction is a unit vector. A perspective ray starts at `position`, so `t` is the distance from the camera. An orthographic ray starts on the near plane and stays parallel to the look direction. `examples/picking_ray.rs` highlights the box under the mouse. `examples/ray_cast.rs` still builds its ray by hand.

```rust
use macroquad::camera::{Camera3D, Projection};
use macroquad::math::{vec2, vec3, Vec3};

let camera = Camera3D {
    position: vec3(0.0, 0.0, 5.0),
    target: Vec3::ZERO,
    up: Vec3::Y,
    fovy: 90.0_f32.to_radians(),
    aspect: Some(1.0),
    projection: Projection::Perspective,
    viewport: Some((0, 0, 200, 200)),
    ..Default::default()
};
let ray = camera.screen_to_ray(vec2(100.0, 100.0));
assert_eq!(ray.origin, camera.position);
assert!(ray.direction.dot(-Vec3::Z) > 0.999);
```

## Planes

`Plane` is `normal · x = distance`. `normal` must be a unit vector for the queries. `Plane::new` stores the values as given and does not normalize. `from_normal_point` and `from_points` do. `from_points` uses `(b - a) × (c - a)`, right-handed.

`signed_distance` is positive on the side the normal points into. `side` returns `PlaneSide::Front`, `Back`, or `Coplanar`. `segment_intersect` returns the crossing point of a closed segment, or `None` when the segment stays strictly on one side. A ray parallel to the plane misses unless the origin is already on it.

```rust
use macroquad::math::{Plane, PlaneSide, Vec3, vec3};

let ground = Plane::new(Vec3::Y, 0.0);
assert_eq!(ground.side(vec3(1.0, 2.0, 3.0)), PlaneSide::Front);
assert_eq!(ground.closest_point(vec3(1.0, 2.0, 3.0)), vec3(1.0, 0.0, 3.0));

let hit = ground
    .ray_intersect(vec3(1.0, 4.0, 2.0), -Vec3::Y)
    .expect("downward ray");
assert_eq!(hit.point, vec3(1.0, 0.0, 2.0));
```

The reported normal is the plane normal. It is not flipped to face the ray.

## Triangles

`Triangle` is three corners. The winding `a → b → c` is the right-hand rule, the same order as `Plane::from_points`. A zero-area triangle contains nothing and the ray misses. `contains` requires the point to lie on the plane, including the boundary. `aabb` is `Aabb::from_points` of the three corners.

`ray_intersect` is Möller–Trumbore and keeps both winding orders. The hit normal is the winding normal, not flipped to face the ray. `ray_intersect_cull_backface` drops a hit whose direction travels with that normal (`normal · direction > 0`).

```rust
use macroquad::math::{Triangle, Vec3, vec3};

let triangle = Triangle::new(Vec3::ZERO, Vec3::X, Vec3::Y);
assert!(triangle.contains(vec3(0.25, 0.25, 0.0)));
assert_eq!(triangle.plane().expect("area").normal, Vec3::Z);

let front = triangle.ray_intersect(vec3(0.2, 0.2, 1.0), -Vec3::Z);
assert!(front.is_some());
let back = triangle.ray_intersect_cull_backface(vec3(0.2, 0.2, -1.0), Vec3::Z);
assert!(back.is_none());
```

`Ray::closest_triangle` matches the other slice queries.

## Oriented boxes

`Obb` is a center, `half_extents`, and a rotation quaternion. A negative half extent is empty. Queries normalize a non-zero finite quaternion. A zero quaternion misses.

`from_world_matrix` decomposes a `Mat4`. The translation is the center. Half extents are half the absolute scale, so the scale vector is the full size passed to `draw_cube`. Shear is dropped. A negative scale stays a positive extent.

```rust
use macroquad::math::{Aabb, Mat4, Obb, Quat, Sphere, Vec3, vec3};

let world = Mat4::from_scale_rotation_translation(
    vec3(2.0, 1.0, 0.5),
    Quat::IDENTITY,
    vec3(0.0, 0.5, 0.0),
);
let box_ = Obb::from_world_matrix(world);
assert!(box_.contains(vec3(0.9, 0.5, 0.0)));

let floor = Aabb::from_center_size(Vec3::ZERO, vec3(4.0, 0.2, 4.0));
assert!(box_.overlaps_aabb(&floor));
assert!(box_.overlaps_sphere(&Sphere::new(vec3(1.2, 0.5, 0.0), 0.3)));
```

`overlaps_aabb` is a separating-axis test. The AABB must be normalized; an inverted one overlaps nothing. `ray_intersect` transforms the ray into box space and uses `Aabb::ray_intersect`, then rotates the point and normal back. `t` is unchanged by that rotation. `Ray::closest_obb` scans a slice the same way as `closest_aabb`.

## Capsule

`Capsule` is the segment from `a` to `b` swept by `radius`. Those endpoints are the inner segment: the rounded ends stick out past them by `radius`. Swapping `a` and `b` does not change the shape. Nothing here picks an up axis. A standing character is just a segment aligned with whatever the game calls up.

A negative or non-finite radius is empty. A radius of zero is the segment itself. `closest_point` on an empty capsule returns the closest point on that segment, not a point out in space.

`overlaps` compares the distance between the two inner segments with the sum of the radii. `overlaps_sphere` does the same with the sphere center. `overlaps_aabb` needs a normalized box; an inverted box overlaps nothing. `ray_intersect` returns the nearer hit on the cylinder or on a spherical cap. An origin already inside reports `t == 0` and a zero normal, same as `Sphere`.

```rust
use macroquad::math::{Aabb, Capsule, Sphere, vec3};

let body = Capsule::new(vec3(0.0, 0.5, 0.0), vec3(0.0, 1.5, 0.0), 0.5);
assert!(body.contains(vec3(0.0, 0.0, 0.0)));

let floor = Aabb::from_center_size(vec3(0.0, -0.1, 0.0), vec3(4.0, 0.2, 4.0));
assert!(body.overlaps_aabb(&floor));
assert!(body.overlaps_sphere(&Sphere::new(vec3(1.0, 0.5, 0.0), 0.5)));
```

`Ray::closest_capsule` matches the other slice queries. `examples/capsule_overlap.rs` moves one with the arrow keys. The wireframe in that example is local; `draw_capsule_wires` is a later item.

## Contact

`Contact` is one overlapping pair: `normal`, `penetration`, and `point`. It does not move either shape. The normal points from the second shape toward the first. A pair that only touches still returns a contact with penetration `0`, so a sphere sitting on a floor is a contact. Separated shapes return `None`.

`Aabb::contact`, `Sphere::contact`, `Sphere::contact_aabb`, `Capsule::contact_sphere`, and `Capsule::contact_aabb` are the queries. Mass and impulses are not here. A capsule whose inner segment runs through a thin box is separated on the short face, not along the segment.

```rust
use macroquad::math::{Aabb, Sphere, Vec3, vec3};

let ball = Sphere::new(vec3(0.0, 0.5, 0.0), 0.5);
let floor = Aabb::from_min_max(vec3(-1.0, -1.0, -1.0), vec3(1.0, 0.0, 1.0));
let contact = ball.contact_aabb(&floor).unwrap();
assert_eq!(contact.penetration, 0.0);
assert_eq!(contact.normal, Vec3::Y);
```

## Sweep

`SweepHit` is the first time a moving box or sphere meets one static box along a delta. `t` is the fraction of that delta, in `0..=1`. `normal` points out of the static box. `point` lies on the static box.

`Aabb::sweep` moves a box. `Sphere::sweep_aabb` moves a sphere, including hits on an edge or a corner. A start that only touches, and a delta that does not move further in, returns `None`, so a shape resting on a floor can still sweep sideways. An overlap deeper than about `1e-4` is already inside and returns `t == 0`. A discrete `overlaps` test can jump a thin wall; the sweep still reports the crossing.

```rust
use macroquad::math::{Aabb, Sphere, vec3, Vec3};

let wall = Aabb::from_min_max(vec3(2.0, 0.0, 0.0), vec3(3.0, 1.0, 1.0));
let body = Aabb::from_min_max(vec3(0.0, 0.0, 0.0), vec3(1.0, 1.0, 1.0));
let hit = body.sweep(vec3(2.0, 0.0, 0.0), &wall).unwrap();
assert_eq!(hit.t, 0.5);
assert_eq!(hit.normal, -Vec3::X);

let ball = Sphere::new(vec3(0.0, 0.5, 0.0), 0.5);
let floor = Aabb::from_min_max(vec3(-1.0, -1.0, -1.0), vec3(1.0, 0.0, 1.0));
assert!(ball.sweep_aabb(vec3(1.0, 0.0, 0.0), &floor).is_none());
```

`examples/sweep_hit.rs` moves a box and a sphere into a wall. One static box does not slide on its own: the example keeps the part of the delta that lies on the hit face. `slide_sphere` does that for a sphere against several boxes.

## Sphere slide

`slide_sphere(center, radius, delta, aabbs, obbs)` returns the new center. The sphere moves by `delta`, stops on an `Aabb` or `Obb`, and keeps the motion along that face. A sphere that only touches a floor still strafes, so the caller does not shrink the radius. A center that starts inside a box is pushed out along that box's shortest axis before the move: a world axis for an `Aabb`, one of the box's own axes for an `Obb`. There is no gravity here.

```rust
use macroquad::math::{slide_sphere, Aabb, vec3};

let floor = Aabb::from_center_size(vec3(0.0, -0.5, 0.0), vec3(4.0, 1.0, 4.0));
let moved = slide_sphere(vec3(0.0, 0.5, 0.0), 0.5, vec3(1.0, -0.25, 0.0), &[floor], &[]);
assert!((moved.x - 1.0).abs() < 1e-4);
assert!((moved.y - 0.5).abs() < 1e-4);
```

## Physics

The library does not integrate velocity, apply gravity, or solve contacts. There is no `RigidBody` type. A game keeps the velocity and calls `slide_sphere` for a sphere against boxes. Gravity is added to the velocity first. The slide keeps the part of the step that lies on a face, so a resting sphere still strafes and a wall removes only the component into the wall. Restitution and mass are not applied. `Contact` describes an overlap that already exists; it does not move either shape.

This is application code, not a macroquad type. The floor is one of the solids. A downward step that the slide refuses clears the vertical speed. A step in open air keeps it.

```rust
use macroquad::math::{slide_sphere, Aabb, Vec3, vec3};

fn step_sphere(center: &mut Vec3, velocity: &mut Vec3, solids: &[Aabb], dt: f32) {
    *velocity += vec3(0.0, -9.8, 0.0) * dt;
    let next = slide_sphere(*center, 0.5, *velocity * dt, solids, &[]);
    if velocity.y < 0.0 && next.y > center.y + velocity.y * dt + 1e-4 {
        velocity.y = 0.0;
    }
    *center = next;
}
```

Rejecting a whole step with `overlaps` sticks a sphere that is only touching a floor, because boundaries are inclusive. `slide_sphere` is the call that still moves along that face. A center that starts inside a box is pushed out along the shortest axis before `delta` is applied. Reading `Sphere::contact_aabb` reports that overlap; it does not perform the push.

## Character movement

There is no character controller. `on_ground`, slope limits, and step offsets are not library fields. `Capsule` is the body shape that controller will use. Nothing here moves a capsule: `slide_sphere` takes a sphere, and the sweep queries are `Aabb::sweep` and `Sphere::sweep_aabb` against one static box.

A capsule standing on a floor is a `Contact` with an upward normal and a penetration of `0`. That is the ground test. A wall's normal fails the upward check, so the same query does not treat a wall as a floor. A slope limit is the cutoff on `normal.y` in this function; the library does not store one.

```rust
use macroquad::math::{Aabb, Capsule, vec3};

fn on_ground(body: &Capsule, floor: &Aabb) -> bool {
    let Some(contact) = body.contact_aabb(floor) else {
        return false;
    };
    contact.normal.y > 0.7 && contact.penetration < 0.05
}
```

Wish velocity is still the caller's. Add it to the capsule with `offset`, then test `overlaps_aabb` or `contact_aabb` against each static box. A touch counts as an overlap, so a capsule that stays on the floor is rejected when the floor is in that list. Keep the floor out of the horizontal test, then move vertically on its own. A large step can tunnel through a thin wall. `Aabb::sweep` against one box reports that crossing; a sideways delta that only touches the floor is `None`. Neither query walks a slope or a step offset. Those stay in the game until the character controller exists.

## Drawing

Wire helpers for these shapes are not a separate API yet. `draw_cube_wires(aabb.center(), aabb.size(), color)` matches an `Aabb`. `draw_sphere` / `draw_sphere_wires` match a `Sphere`. `draw_line_3d` draws a ray segment or a triangle edge. An oriented box has to be outlined by the caller; `examples/obb_overlap.rs` does that. `examples/plane_hit.rs` draws a finite patch of a plane with `draw_affine_parallelogram`.

Set a `Camera3D` before any of those calls. The samples use `up: Vec3::Y`.

## Feature flags

The collision types above are always compiled. Parsing and drawing a glTF file require the `gltf` feature, which is not a default feature. See the [Blender tutorial](blender-to-macroquad.md).
