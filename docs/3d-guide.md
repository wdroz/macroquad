# 3D collision guide

This fork adds 3D queries to `macroquad::math`. They are also in the prelude (`use macroquad::prelude::*`). The same types are in `macroquad::math` for a test or a tool that must not open a window.

Nothing here assumes an up axis. `Camera3D::up` stays caller-defined. The examples in this repository use **Y-up**, which matches a glTF file exported from Blender. Sizes are unitless. A glTF scene is in meters, and `get_frame_time` is in seconds, so a gravity of `(0, -9.8, 0)` is meters per second squared when the level is left in those units.

`draw_cube` / `draw_cube_wires` take a center and a **full** size. `Aabb::from_center_size` uses that same pair. `Obb::half_extents` is half of that full size. `Capsule` `a` and `b` are the inner segment, so the hull extends `radius` past each end.

There is no rigid-body world and no character controller in the library yet. The last two sections show the application code those features will replace. The narrative for loading a Blender level is [blender-to-macroquad.md](blender-to-macroquad.md).

## Ray hits

Every ray query returns `Option<RayHit>`:

- `t` is the parameter along `origin + direction * t`. It is a distance only when `direction` is a unit vector.
- `point` is the world position of the hit.
- `normal` is the outward unit normal, except when the origin is already inside a solid (`t == 0`), where it is zero.

Hits with `t < 0` are behind the origin and are rejected. Boundaries are inclusive, same as `Rect`: touching a face, edge, or corner counts. A non-finite origin, direction, or shape misses.

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

`Camera3D` does not unproject a mouse position into a `Ray` yet. Build the ray from the camera position and a direction you already have, as `examples/ray_cast.rs` does.

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

`Aabb::contact`, `Sphere::contact`, `Sphere::contact_aabb`, `Capsule::contact_sphere`, and `Capsule::contact_aabb` are the queries. Mass and impulses are not here.

```rust
use macroquad::math::{Aabb, Sphere, Vec3, vec3};

let ball = Sphere::new(vec3(0.0, 0.5, 0.0), 0.5);
let floor = Aabb::from_min_max(vec3(-1.0, -1.0, -1.0), vec3(1.0, 0.0, 1.0));
let contact = ball.contact_aabb(&floor).unwrap();
assert_eq!(contact.penetration, 0.0);
assert_eq!(contact.normal, Vec3::Y);
```

## Physics

The library does not integrate velocity, apply gravity, or solve contacts. There is no `RigidBody` type. A game that needs a falling body keeps the velocity itself and uses the queries above to accept or reject a step.

This is application code, not a macroquad type. It only handles a floor. It does not slide along a wall, separate an overlap that already exists, or apply restitution. A step that starts overlapping the floor never moves. A step that would enter the floor is discarded, so the body stops a little above the surface instead of snapping to it. Touching already counts as an overlap.

```rust
use macroquad::math::{Aabb, Vec3, vec3};

struct Body {
    shape: Aabb,
    velocity: Vec3,
}

fn step_body(body: &mut Body, floor: &Aabb, dt: f32) {
    let gravity = vec3(0.0, -9.8, 0.0);
    body.velocity += gravity * dt;
    let next = body.shape.offset(body.velocity * dt);
    if next.overlaps(floor) {
        body.velocity.y = body.velocity.y.max(0.0);
    } else {
        body.shape = next;
    }
}
```

`Contact` reports an overlap. It does not assign mass or apply an impulse, and there is no solver. Those stay later roadmap items.

## Character movement

There is no character controller. Sweep tests, `on_ground`, slope limits, and step offsets are not in the library. A capsule body is not in the library either. The pattern below moves a kinematic AABB and blocks the whole step when the next position overlaps a static box. Touching counts as an overlap, so a move that stays in contact with the floor is rejected if the floor is in `static_boxes`. A large step can tunnel through a thin wall, and a slope blocks the move the same way a wall does.

```rust
use macroquad::math::{Aabb, Vec3, vec3};

fn try_move(body: &mut Aabb, wish: Vec3, static_boxes: &[Aabb]) -> bool {
    let next = body.offset(wish);
    if static_boxes.iter().any(|solid| next.overlaps(solid)) {
        return false;
    }
    *body = next;
    true
}

fn on_ground(body: &Aabb, static_boxes: &[Aabb]) -> bool {
    // Inclusive overlap means a body standing on a floor already touches it.
    // Probe the thin slab under the feet instead of shifting the whole body.
    let probe = Aabb::from_min_max(
        vec3(body.min.x, body.min.y - 0.05, body.min.z),
        vec3(body.max.x, body.min.y, body.max.z),
    );
    static_boxes.iter().any(|solid| probe.overlaps(solid))
}
```

Keep the floor out of the horizontal test, then move vertically on its own, if sideways motion should slide along the ground. That split is still a discrete overlap test, not a sweep.

## Drawing

Wire helpers for these shapes are not a separate API yet. `draw_cube_wires(aabb.center(), aabb.size(), color)` matches an `Aabb`. `draw_sphere` / `draw_sphere_wires` match a `Sphere`. `draw_line_3d` draws a ray segment or a triangle edge. An oriented box has to be outlined by the caller; `examples/obb_overlap.rs` does that. `examples/plane_hit.rs` draws a finite patch of a plane with `draw_affine_parallelogram`.

Set a `Camera3D` before any of those calls. The samples use `up: Vec3::Y`.

## Feature flags

The collision types above are always compiled. Parsing and drawing a glTF file require the `gltf` feature, which is not a default feature. See the [Blender tutorial](blender-to-macroquad.md).
