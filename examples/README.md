# Examples added on this fork

The other files in this directory are the upstream macroquad samples. This page lists the 3D collision examples and the glTF example added here.

Collision examples use the default features and a Y-up `Camera3D`. Escape quits each of them. Arrow keys move on the XZ plane: left and right change X, up and down change Z (up is toward -Z).

```sh
cargo run --example aabb_overlap
cargo run --example sphere_overlap
cargo run --example ray_cast
cargo run --example plane_hit
cargo run --example triangle_hit
cargo run --example obb_overlap
cargo run --example capsule_overlap
cargo run --example sweep_hit
cargo run --example sweep_obb
cargo run --example picking_ray
cargo run --example frustum_cull
cargo run --example debug_draw
cargo run --features gltf --example gltf_textured
cargo run --example material_depth_check
```

`material_depth_check` is an OpenGL/WebGL shader regression that exits after checking rendered pixels. It verifies that opaque surfaces hide glass, transparent surfaces keep depth testing without writing depth, subsequent clears reset depth, and changing a custom texture preserves the bindings of earlier queued draws. It also exercises adaptive streaming-buffer growth, reuse with smaller geometry, and recreation after the configured capacity changes.

## `aabb_overlap`

Moves an `Aabb` with the arrow keys. The box turns a different color when it overlaps one of the static boxes. A ray from +Z finds the closest slab hit. Wireframes are `draw_aabb`, the same center and full size as `Aabb::from_center_size`. The ray segment is `draw_ray`.

## `sphere_overlap`

Moves a `Sphere` the same way. The sphere is tested against the static boxes with `overlaps_aabb`. One ray reports the closest hit among the boxes and the sphere. `draw_sphere` uses the sphere's center and radius.

## `ray_cast`

Left and right yaw a `Ray` in the XZ plane. `closest_aabb` and `closest_sphere` reject hits behind the origin. The two hits share a direction, so the example compares them by `hit.t` and marks the winner.

## `plane_hit`

Moves a marker with the arrow keys. `Q` raises it and `E` lowers it. The marker's color is `Plane::side` against the ground (`Plane::new(Vec3::Y, 0.0)`). A ray and a segment are tested against that ground and a plane built with `Plane::from_points`. The finite patch is `draw_affine_parallelogram`.

## `triangle_hit`

Left and right yaw a ray. `Ray::closest_triangle` keeps both winding orders. Edges are `draw_triangle_wires`. The hit normal is the triangle winding normal.

## `obb_overlap`

Arrow keys move an oriented box. The box yaws on its own around Y. `Obb::from_world_matrix` takes the same full size a rotated `draw_cube` would. The example tests `overlaps_aabb`, `overlaps_sphere`, and `ray_intersect`. The outline is `draw_obb`.

## `capsule_overlap`

Moves a vertical `Capsule` with the arrow keys. `a` and `b` are the inner segment; the rounded ends extend past them by `radius`. The capsule turns a different color when `overlaps_aabb` or `overlaps_sphere` hits. One ray reports `Capsule::ray_intersect`. The outline is `draw_capsule_wires`: sphere wires at the inner ends and four lines on the hull.

## `sweep_hit`

Moves a box and a sphere with the arrow keys into one wall. `Aabb::sweep` and `Sphere::sweep_aabb` stop the shape at `SweepHit::t` and the example keeps the leftover motion along `normal`, so holding into the wall slides along it. A pure touch is not an impact. Escape quits.

## `sweep_obb`

Moves a sphere with the arrow keys into a yawed cover. `Sphere::sweep_obb` stops it at `SweepHit::t` and the example keeps the leftover motion along `normal`. The cover's thin half-extent is 0.225, so a discrete overlap test can jump it; the sweep still reports the face. A pure touch is not an impact. Escape quits.

## `picking_ray`

Moves the mouse. `Camera3D::screen_to_ray` unprojects the cursor with the same view-projection the example passes to `set_camera`. `Ray::closest_aabb` marks the nearest box. The camera is Y-up. Escape quits.

## `frustum_cull`

Looks at a grid of boxes from the side. The yellow wireframe is a second `Camera3D`. `Camera3D::frustum` builds a `Frustum`, and `intersects_aabb` turns the boxes it meets green. Left and right yaw that camera. Space hides boxes outside it. Escape quits.

## `debug_draw`

A static scene of one of each wireframe: `draw_aabb`, `draw_obb`, `draw_capsule_wires`, `draw_triangle_wires`, `draw_plane_wires`, and `draw_ray`. `draw_plane_wires` takes the full side length and projects its center onto the plane. `draw_plane` is still the XZ quad. Escape quits.

## `gltf_textured`

Requires `--features gltf`. With no argument it draws `assets/textured_quad.glb`, a checkerboard quad. An argument is a `.glb` path passed to `load_file`, then `GltfModel::from_bytes`. The camera orbits the model's bounds with `up: Vec3::Y`. `model.draw()` submits the textured chunks. The on-screen line is `model.stats`.

`gltf_textured` still builds without the feature: that binary prints a message and exits. It does not parse a file unless `gltf` is enabled.

## Assets

| Path | Role |
| --- | --- |
| `assets/triangle.gltf`, `assets/triangle.glb` | Hierarchy, box collider, spawn, and a 1×1 base-color image used by the loader tests. Regenerate with `assets/gen_triangle_gltf.py`. |
| `assets/textured_quad.glb` | Checkerboard drawn by `gltf_textured`. Regenerate with `assets/gen_textured_quad.py`. |

The guides are [`docs/3d-guide.md`](../docs/3d-guide.md) and [`docs/blender-to-macroquad.md`](../docs/blender-to-macroquad.md).
