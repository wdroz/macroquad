# 3D roadmap

Enhancements since last fixer round: 2
Fixer rounds since last game round: 0

Source of truth for the unattended 3D enhancement bot on this fork. The goal is to make 3D games easier to build with macroquad: collision primitives, raycasting, simple rigid-body physics, collision response, a character controller, and spatial partitioning.

Macroquad stays a thin, beginner-friendly layer over `glam` (already re-exported from `macroquad::math` and the prelude). Primitives are `Copy` structs with public fields, `const fn` where `Rect` is `const`, and no new crate dependencies. Do not break existing public APIs. Do not add GitHub Actions or any other CI. Verify on the VM with `cargo build`, `cargo test`, `cargo clippy`, and `cargo build --example <name>` for any new example. Push straight to `master`. Never force-push.

## How a round uses this file

- The counter line above is authoritative. `Fixer rounds since last game round` is the second counter. Enhancement rounds leave it unchanged. When it is 2, the round is a game round instead of an enhancement or a fixer; a game round sets it back to 0.
- If the counter is **below 5** and this is not a game round, this is an **enhancement** round: implement **only** the first unchecked item, in the style of the surrounding code. Then tick it, append a dated log line, and increment the counter by 1.
- If the counter is **5 or more**, this is a **fixer** round: do not add a feature. Review the work logged since the previous fixer entry, fix bugs, doc gaps, missing tests, and API inconsistencies, then set the counter back to 0 and append a fixer log line.
- Keep each enhancement small enough to review as one commit. Leave later items alone even if they would be easy to fold in.
- Math primitives live in `macroquad::math` and show up through `use macroquad::prelude::*`. Types that need the GPU or the window (`draw_*`, camera unprojection) live next to the existing draw/camera code. Stateful worlds (rigid bodies, character controller, grids) can live in their own module once the pure queries they depend on exist.
- Boxes and rays are axis-aligned in **world space**. Do not assume Y-up or Z-up; `Camera3D::up` is caller-defined. `draw_cube` / `draw_cube_wires` take a center and a full size; `Aabb::from_center_size` matches that.

## Pain points from game rounds

(none yet)

## Priorities

- [x] **AABB** — `Aabb` in `macroquad::math`: min/max corners, `from_center_size` (same center and full size as `draw_cube`), `from_points`, containment, overlap, intersection, union, closest point, expand/normalize, and a slab `ray_intersect` returning `RayHit` (`t`, `point`, outward `normal`). Inclusive boundaries, same as `Rect`.
- [x] **Sphere** — `Sphere` in `macroquad::math` (center + radius, mirroring `Circle`). `contains`, overlaps sphere, overlaps `Aabb`, closest point, `ray_intersect` → `RayHit`. No solver and no new ray type yet.
- [ ] **Ray** — `Ray` { origin, direction } with `point_at(t)`, normalization helper, and closest-hit queries against a slice of `Aabb` and a slice of `Sphere`, reusing `RayHit`. Reject `t < 0`. Do not unproject from the camera in this item.
- [ ] **Plane** — `Plane` from unit normal + signed distance, and from three points. Signed distance, which-side test, closest point, `ray_intersect` → `RayHit`, and segment intersection (endpoints on opposite sides).
- [ ] **Triangle** — `Triangle` { a, b, c }. Barycentric `contains`, plane of the triangle, AABB via `Aabb::from_points`, closest point, and Möller–Trumbore `ray_intersect` → `RayHit` (both winding orders count; provide a flag or separate method for backface culling).
- [ ] **OBB** — `Obb` { center, half_extents, rotation: `Quat` }. `overlaps` AABB (separating axis), overlaps sphere, `contains` point, `ray_intersect` by transforming the ray into box space and delegating to `Aabb`. World-matrix helper (`Mat4`) so a rotated `draw_cube` can share the same pose later.
- [ ] **Capsule** — `Capsule` { a, b, radius } as a segment swept by a radius. `contains`, overlaps sphere, overlaps `Aabb`, overlaps capsule, closest point, `ray_intersect` → `RayHit`. This is the body shape the character controller will use.
- [ ] **Contact** — `Contact` { normal, penetration, point } for overlapping pairs only: AABB–AABB, sphere–sphere, sphere–AABB, capsule–sphere, capsule–AABB. Normal points from the second shape toward the first (or document the opposite and test it). No impulses and no position correction yet. Return `None` when the shapes are separated.
- [ ] **Sweep** — continuous query: moving AABB vs static AABB, and moving sphere vs static AABB, along a delta. Return time of impact in `0..=1` plus the hit normal, or `None` if the path misses. Start-overlapping reports `t = 0`. Needed before the character controller.
- [ ] **Picking ray** — `Camera3D` method that unprojects a screen point (typically the mouse) into a world `Ray` using the camera's view-projection. Viewport and render-target dimensions must match `world_to_screen`'s existing rules. Unit-test with a synthetic perspective camera aimed down an axis, without opening a window.
- [ ] **Frustum** — extract six planes from a `Camera3D` view-projection matrix. `contains` / `intersects` tests for a point, a `Sphere`, and an `Aabb` (the AABB test may be conservative). Used for culling, not for collision response.
- [ ] **Debug draw** — `draw_aabb`, `draw_sphere` already exists, `draw_ray` (segment to `t`), `draw_plane` (small quad), `draw_triangle_wires`, `draw_obb`, `draw_capsule_wires`. Implement with the current `draw_line_3d` / `draw_cube_wires` / `draw_sphere_wires` paths. No new shaders.
- [ ] **Uniform grid** — `UniformGrid` broadphase: cell size, insert `Aabb` (objects may span cells), query overlap against an `Aabb` or a `Ray`, clear/rebuild. Store caller indices (`u32`), not closures or trait objects, so the API stays beginner-friendly. No BVH in this item.
- [ ] **Rigid body** — kinematic/dynamic `RigidBody` with position, linear velocity, mass (zero = static), and a shape enum of AABB or sphere. `integrate(dt, gravity)` applies gravity, damping, and velocity. No rotation, no contacts, no islands.
- [ ] **Collision response** — one discrete step: detect overlaps between rigid bodies (use the grid when it helps, a nested loop is fine at this size), build `Contact`s, separate positions by mass, and apply an impulse along the contact normal with optional restitution. Static mass (`0`) does not move. Leave continuous sweep for the character controller.
- [ ] **Character controller** — kinematic capsule or AABB body. Move with a caller-supplied velocity, collide against a slice of static AABBs using the sweep test, snap to ground, and expose `on_ground`, ground normal, and a slope limit in degrees. Step offset is in this item; moving platforms are not.
- [ ] **Triangle mesh collider** — a list of `Triangle`s plus a bounding `Aabb` per triangle (or a tiny top-level AABB). `ray_intersect` returns the closest `RayHit` and triangle index. Overlap vs `Aabb` and `Sphere`. Linear scan is acceptable; do not build a BVH here.
- [ ] **Heightfield** — regular grid of heights (row-major `Vec<f32>`, scale, origin). Sample height at `(x, z)`, `Aabb` overlap, and `ray_intersect`. Axis of "up" is a parameter (`Vec3`), defaulting to `Vec3::Y` only as a default value, not a hardcoded assumption inside the math.
- [ ] **Octree** — static octree of AABBs addressed by `u32` indices, built once, queried by AABB overlap and by ray. Fixed depth cap so a degenerate cluster cannot recurse forever. This does not replace `UniformGrid`; it is the static-scene alternative.
- [ ] **Distance joint** — constraint between two `RigidBody` positions: rest length, Baumgarte or a single position/velocity correction per step. No angular joints, no full solver stack.

## Log

- 2026-10-17 — Enhancement: Created this roadmap and added `Aabb` / `RayHit` in `macroquad::math` (containment, overlap, intersection, union, closest point, slab ray). Example `examples/aabb_overlap.rs`. Counter 1.
- 2026-10-17 — Enhancement: added `Sphere` in `macroquad::math` (contains, overlaps sphere and AABB, closest point, `ray_intersect` → `RayHit`). Example `examples/sphere_overlap.rs`. Counters: N=2, M=0.
