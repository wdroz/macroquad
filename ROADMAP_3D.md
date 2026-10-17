# 3D roadmap

Enhancements since last fixer round: 3
Fixer rounds since last game round: 0

Source of truth for the unattended 3D enhancement bot on this fork. The goal is to make 3D games easier to build with macroquad: collision primitives, raycasting, simple rigid-body physics, collision response, a character controller, and spatial partitioning. A second track loads a Blender scene exported as glTF 2.0 (meshes, materials, textures, node transforms, and collider / trigger / spawn markup) so a level does not have to be rebuilt by hand.

Macroquad stays a thin, beginner-friendly layer over `glam` (already re-exported from `macroquad::math` and the prelude). Primitives are `Copy` structs with public fields, `const fn` where `Rect` is `const`, and no new crate dependencies on the default feature set. The `gltf` crate is allowed behind the `gltf` feature (see the Blender asset track). Do not break existing public APIs. Do not add GitHub Actions or any other CI. Verify on the VM with `cargo build`, `cargo test`, `cargo clippy`, and `cargo build --example <name>` for any new example. Push straight to `master`. Never force-push.

## How a round uses this file

- The counter line above is authoritative. `Fixer rounds since last game round` is the second counter. Enhancement rounds leave it unchanged. When it is 2, the round is a game round instead of an enhancement or a fixer; a game round sets it back to 0.
- If the counter is **below 5** and this is not a game round, this is an **enhancement** round: implement **only** the first unchecked item, in the style of the surrounding code. Then tick it, append a dated log line, and increment the counter by 1.
- If the counter is **5 or more**, this is a **fixer** round: do not add a feature. Review the work logged since the previous fixer entry, fix bugs, doc gaps, missing tests, and API inconsistencies, then set N back to 0, increase M by 1, and append a fixer log line.
- Keep each enhancement small enough to review as one commit. Leave later items alone even if they would be easy to fold in.
- Math primitives live in `macroquad::math` and show up through `use macroquad::prelude::*`. Types that need the GPU or the window (`draw_*`, camera unprojection) live next to the existing draw/camera code. Stateful worlds (rigid bodies, character controller, grids) can live in their own module once the pure queries they depend on exist.
- Boxes and rays are axis-aligned in **world space**. Do not assume Y-up or Z-up; `Camera3D::up` is caller-defined. `draw_cube` / `draw_cube_wires` take a center and a full size; `Aabb::from_center_size` matches that.

## Pain points from game rounds

(none yet)

## Blender asset track

Someone builds a scene in Blender (by hand or through an AI-driven Blender MCP server), exports glTF 2.0 (`.glb` or `.gltf`), and a game loads meshes, materials, textures, and the node hierarchy. Colliders, triggers, and spawn points are marked in Blender and become this library's shapes.

The checkboxes live in **Priorities**, interleaved with the collision items they depend on. This section is the convention those items share. Do not implement from this section ahead of the first unchecked priority.

- Cargo feature `gltf` (not a default feature) depends on the `gltf` crate. Parsing bytes must not open a window. Types that need the GPU (`Mesh`, `Texture2D`) stay next to the existing draw code and are their own item.
- glTF is Y-up, right-handed, meters. Blender is Z-up; Blender's glTF exporter converts Z-up to Y-up on export. The loader stores glTF axes as written and does not apply a second conversion. A later helper reorients a loaded scene onto a caller-chosen up axis. Macroquad does not assume an up axis (`Camera3D::up` is caller-defined). Examples in this repo use Y-up, which matches an exported glTF file.
- Node world transforms are the glTF hierarchy (translation, rotation, scale).
- Markup, preferred source first:
  - glTF node `extras`: `mq_shape` = `box` | `sphere` | `capsule`, `mq_role` = `collider` | `trigger` | `spawn`.
  - Otherwise a name suffix: `_collider`, `_trigger`, `_spawn`.
- A `box` uses the node translation as the center and the node's full scale as the AABB size, matching `draw_cube` / `Aabb::from_center_size`. A `sphere` uses the translation as the center and the largest scale component as the radius. `capsule` waits until `Capsule` exists: local Y is the segment (glTF Y-up), scale X/Z is the radius, scale Y is the distance from center to an endpoint of the inner segment.
- Empties (nodes with no mesh) are the usual markers. A mesh with a suffix and no `mq_shape` becomes an AABB around its transformed positions.
- Checked-in bytes live under `examples/assets/`. A Python script headless Blender can run is how to regenerate them. Tests and examples must not require Blender to be installed.

## Priorities

- [x] **AABB** — `Aabb` in `macroquad::math`: min/max corners, `from_center_size` (same center and full size as `draw_cube`), `from_points`, containment, overlap, intersection, union, closest point, expand/normalize, and a slab `ray_intersect` returning `RayHit` (`t`, `point`, outward `normal`). Inclusive boundaries, same as `Rect`.
- [x] **Sphere** — `Sphere` in `macroquad::math` (center + radius, mirroring `Circle`). `contains`, overlaps sphere, overlaps `Aabb`, closest point, `ray_intersect` → `RayHit`. No solver and no new ray type yet.
- [x] **Ray** — `Ray` { origin, direction } with `point_at(t)`, normalization helper, and closest-hit queries against a slice of `Aabb` and a slice of `Sphere`, reusing `RayHit`. Reject `t < 0`. Do not unproject from the camera in this item.
- [ ] **Plane** — `Plane` from unit normal + signed distance, and from three points. Signed distance, which-side test, closest point, `ray_intersect` → `RayHit`, and segment intersection (endpoints on opposite sides).
- [ ] **glTF scene** — feature `gltf`. Parse `.glb` / `.gltf` bytes into a `GltfScene`: nodes (name, parent, local translation / rotation / scale, world `Mat4`) and meshes (positions, indices, uvs, normals, per-primitive base-color factor). No images, no GPU, no colliders. Ship a tiny checked-in file under `examples/assets/` and unit-test the hierarchy against it. Axes as in the Blender asset track.
- [ ] **Triangle** — `Triangle` { a, b, c }. Barycentric `contains`, plane of the triangle, AABB via `Aabb::from_points`, closest point, and Möller–Trumbore `ray_intersect` → `RayHit` (both winding orders count; provide a flag or separate method for backface culling).
- [ ] **glTF markers** — read `extras` and name suffixes (see the Blender asset track) into colliders (`Aabb`, `Sphere`), triggers (same shapes, flagged), and spawn points (`Vec3` + name). Ignore `capsule` until the later shape item. Extend the sample asset with one box collider empty and one spawn. Unit-test without a window.
- [ ] **glTF draw** — build macroquad `Mesh` values from a `GltfScene`, vertex-colored from the base-color factor. Lives next to `models` because it needs the GPU. Example draws the sample. Marker empties have no mesh, so they are not drawn.
- [ ] **OBB** — `Obb` { center, half_extents, rotation: `Quat` }. `overlaps` AABB (separating axis), overlaps sphere, `contains` point, `ray_intersect` by transforming the ray into box space and delegating to `Aabb`. World-matrix helper (`Mat4`) so a rotated `draw_cube` can share the same pose later.
- [ ] **Capsule** — `Capsule` { a, b, radius } as a segment swept by a radius. `contains`, overlaps sphere, overlaps `Aabb`, overlaps capsule, closest point, `ray_intersect` → `RayHit`. This is the body shape the character controller will use.
- [ ] **glTF up-axis** — reorient a `GltfScene` so glTF +Y maps onto a caller-supplied up axis (`Vec3`). `Vec3::Y` is a no-op, which is the Blender-exporter case. Test that a Z-up result maps glTF Y onto Z and that markers move with the meshes. Do not change the default loader.
- [ ] **Contact** — `Contact` { normal, penetration, point } for overlapping pairs only: AABB–AABB, sphere–sphere, sphere–AABB, capsule–sphere, capsule–AABB. Normal points from the second shape toward the first (or document the opposite and test it). No impulses and no position correction yet. Return `None` when the shapes are separated.
- [ ] **Sweep** — continuous query: moving AABB vs static AABB, and moving sphere vs static AABB, along a delta. Return time of impact in `0..=1` plus the hit normal, or `None` if the path misses. Start-overlapping reports `t = 0`. Needed before the character controller.
- [ ] **Picking ray** — `Camera3D` method that unprojects a screen point (typically the mouse) into a world `Ray` using the camera's view-projection. Viewport and render-target dimensions must match `world_to_screen`'s existing rules. Unit-test with a synthetic perspective camera aimed down an axis, without opening a window.
- [ ] **glTF materials** — embedded images on the scene as raw RGBA (`width`, `height`, `pixels`) plus the texture index each primitive uses. No `Texture2D` in the pure type. Extend the sample with a 1×1 image and test the bytes.
- [ ] **Frustum** — extract six planes from a `Camera3D` view-projection matrix. `contains` / `intersects` tests for a point, a `Sphere`, and an `Aabb` (the AABB test may be conservative). Used for culling, not for collision response.
- [ ] **Debug draw** — `draw_aabb`, `draw_sphere` already exists, `draw_ray` (segment to `t`), `draw_plane` (small quad), `draw_triangle_wires`, `draw_obb`, `draw_capsule_wires`. Implement with the current `draw_line_3d` / `draw_cube_wires` / `draw_sphere_wires` paths. No new shaders.
- [ ] **Uniform grid** — `UniformGrid` broadphase: cell size, insert `Aabb` (objects may span cells), query overlap against an `Aabb` or a `Ray`, clear/rebuild. Store caller indices (`u32`), not closures or trait objects, so the API stays beginner-friendly. No BVH in this item.
- [ ] **Rigid body** — kinematic/dynamic `RigidBody` with position, linear velocity, mass (zero = static), and a shape enum of AABB or sphere. `integrate(dt, gravity)` applies gravity, damping, and velocity. No rotation, no contacts, no islands.
- [ ] **glTF collider shapes** — honor `mq_shape = capsule` as a `Capsule`, and a `box` whose node rotation is not axis-aligned as an `Obb`. Axis-aligned boxes stay `Aabb`. Document the rule and test one rotated box plus one capsule empty.
- [ ] **Collision response** — one discrete step: detect overlaps between rigid bodies (use the grid when it helps, a nested loop is fine at this size), build `Contact`s, separate positions by mass, and apply an impulse along the contact normal with optional restitution. Static mass (`0`) does not move. Leave continuous sweep for the character controller.
- [ ] **Character controller** — kinematic capsule or AABB body. Move with a caller-supplied velocity, collide against a slice of static AABBs using the sweep test, snap to ground, and expose `on_ground`, ground normal, and a slope limit in degrees. Step offset is in this item; moving platforms are not.
- [ ] **glTF room** — a Python script a headless Blender can run that builds a room (floor, walls, a trigger, a spawn, a couple of collider empties) and exports `examples/assets/room.glb`. Commit the bytes so Blender is not required later. Example loads the room, draws it, and uses the imported markers. The script should look like the calls an AI Blender MCP server would make.
- [ ] **Triangle mesh collider** — a list of `Triangle`s plus a bounding `Aabb` per triangle (or a tiny top-level AABB). `ray_intersect` returns the closest `RayHit` and triangle index. Overlap vs `Aabb` and `Sphere`. Linear scan is acceptable; do not build a BVH here.
- [ ] **Heightfield** — regular grid of heights (row-major `Vec<f32>`, scale, origin). Sample height at `(x, z)`, `Aabb` overlap, and `ray_intersect`. Axis of "up" is a parameter (`Vec3`), defaulting to `Vec3::Y` only as a default value, not a hardcoded assumption inside the math.
- [ ] **Octree** — static octree of AABBs addressed by `u32` indices, built once, queried by AABB overlap and by ray. Fixed depth cap so a degenerate cluster cannot recurse forever. This does not replace `UniformGrid`; it is the static-scene alternative.
- [ ] **Distance joint** — constraint between two `RigidBody` positions: rest length, Baumgarte or a single position/velocity correction per step. No angular joints, no full solver stack.

## Log

- 2026-10-17 — Enhancement: Created this roadmap and added `Aabb` / `RayHit` in `macroquad::math` (containment, overlap, intersection, union, closest point, slab ray). Example `examples/aabb_overlap.rs`. Counter 1.
- 2026-10-17 — Enhancement: added `Sphere` in `macroquad::math` (contains, overlaps sphere and AABB, closest point, `ray_intersect` → `RayHit`). Example `examples/sphere_overlap.rs`. Counters: N=2, M=0.
- 2026-10-17 — Enhancement: added `Ray` / `ClosestHit` in `macroquad::math` (`point_at`, `normalized`, `closest_aabb`, `closest_sphere`; reject `t < 0`). Example `examples/ray_cast.rs`. Interleaved the Blender glTF track into Priorities. Counters: N=3, M=0.
