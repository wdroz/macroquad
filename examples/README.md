# Examples added on this fork

The other files in this directory are the upstream macroquad samples. This page is only the 3D collision examples and the glTF example added here. There is no game under `examples/games/` yet.

Collision examples use the default features and a Y-up `Camera3D`. Escape quits each of them. Arrow keys move on the XZ plane: left and right change X, up and down change Z (up is toward -Z).

```sh
cargo run --example aabb_overlap
cargo run --example sphere_overlap
cargo run --example ray_cast
cargo run --example plane_hit
cargo run --example triangle_hit
cargo run --example obb_overlap
cargo run --example capsule_overlap
cargo run --features gltf --example gltf_textured
```

## `aabb_overlap`

Moves an `Aabb` with the arrow keys. The box turns a different color when it overlaps one of the static boxes. A ray from +Z finds the closest slab hit. Wireframes use `draw_cube_wires(center, size)`, the same center and full size as `Aabb::from_center_size`.

## `sphere_overlap`

Moves a `Sphere` the same way. The sphere is tested against the static boxes with `overlaps_aabb`. One ray reports the closest hit among the boxes and the sphere. `draw_sphere` uses the sphere's center and radius.

## `ray_cast`

Left and right yaw a `Ray` in the XZ plane. `closest_aabb` and `closest_sphere` reject hits behind the origin. The two hits share a direction, so the example compares them by `hit.t` and marks the winner.

## `plane_hit`

Moves a marker with the arrow keys. `Q` raises it and `E` lowers it. The marker's color is `Plane::side` against the ground (`Plane::new(Vec3::Y, 0.0)`). A ray and a segment are tested against that ground and a plane built with `Plane::from_points`. The finite patch is `draw_affine_parallelogram`.

## `triangle_hit`

Left and right yaw a ray. `Ray::closest_triangle` keeps both winding orders. Edges are `draw_line_3d`. The hit normal is the triangle winding normal.

## `obb_overlap`

Arrow keys move an oriented box. The box yaws on its own around Y. `Obb::from_world_matrix` takes the same full size a rotated `draw_cube` would. The example tests `overlaps_aabb`, `overlaps_sphere`, and `ray_intersect`, and draws its own wire outline. There is no `draw_obb` yet.

## `capsule_overlap`

Moves a vertical `Capsule` with the arrow keys. `a` and `b` are the inner segment; the rounded ends extend past them by `radius`. The capsule turns a different color when `overlaps_aabb` or `overlaps_sphere` hits. One ray reports `Capsule::ray_intersect`. The wires are the example's own spheres along the segment. There is no `draw_capsule_wires` yet.

## `gltf_textured`

Requires `--features gltf`. With no argument it draws `assets/textured_quad.glb`, a checkerboard quad. An argument is a `.glb` path passed to `load_file`, then `GltfModel::from_bytes`. The camera orbits the model's bounds with `up: Vec3::Y`. `model.draw()` submits the textured chunks. The on-screen line is `model.stats`.

`gltf_textured` still builds without the feature: that binary prints a message and exits. It does not parse a file unless `gltf` is enabled.

## Assets

| Path | Role |
| --- | --- |
| `assets/triangle.gltf`, `assets/triangle.glb` | Hierarchy, box collider, and spawn used by the loader tests. Regenerate with `assets/gen_triangle_gltf.py`. |
| `assets/textured_quad.glb` | Checkerboard drawn by `gltf_textured`. Regenerate with `assets/gen_textured_quad.py`. |

The guides are [`docs/3d-guide.md`](../docs/3d-guide.md) and [`docs/blender-to-macroquad.md`](../docs/blender-to-macroquad.md).
