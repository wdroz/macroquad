# Examples added on this fork

The other files in this directory are the upstream macroquad samples. This page lists the 3D collision examples, the glTF example, and the games added here.

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
cargo run --features gltf --example gltf_textured
cargo run --features gltf --example marble_maze
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

## `sweep_hit`

Moves a box and a sphere with the arrow keys into one wall. `Aabb::sweep` and `Sphere::sweep_aabb` stop the shape at `SweepHit::t` and the example keeps the leftover motion along `normal`, so holding into the wall slides along it. A pure touch is not an impact. Escape quits.

## `gltf_textured`

Requires `--features gltf`. With no argument it draws `assets/textured_quad.glb`, a checkerboard quad. An argument is a `.glb` path passed to `load_file`, then `GltfModel::from_bytes`. The camera orbits the model's bounds with `up: Vec3::Y`. `model.draw()` submits the textured chunks. The on-screen line is `model.stats`.

`gltf_textured` still builds without the feature: that binary prints a message and exits. It does not parse a file unless `gltf` is enabled. `marble_maze` does the same.

## `marble_maze`

Requires `--features gltf`. A rolling-ball maze. The level is `assets/marble_maze.glb` (regenerate with Blender: `blender --background --python assets/gen_marble_maze.py`). The marble starts at `player_spawn`. Arrow keys or WASD accelerate it on the XZ plane. Up and W roll toward -Z. Gravity is applied in the example. R returns to the spawn. The gold ring is the `goal_trigger` sphere. Escape quits.

The chase camera sets `up: Vec3::Y`. `Camera3D`'s default up is +Z, and this file is Y-up, so the example does not call `GltfScene::with_up_axis` or `GltfModel::with_up_axis`. Walls are mesh nodes whose names end in `_collider` and have no `mq_shape`, so a rotated mesh is still the world `Aabb` of its positions. The example builds an `Obb` from that node's world matrix. A node with `mq_shape` = `box` would already be an `Obb` sized to the mesh. The overlap probe is 0.02 smaller than the drawn marble. `slide_sphere` accepts the full radius; this example keeps the probe it shipped with. Falling below the floor returns to the spawn. The goal sphere is drawn by the example; the imported trigger is not a mesh.

## Assets

| Path | Role |
| --- | --- |
| `assets/triangle.gltf`, `assets/triangle.glb` | Hierarchy, box collider, and spawn used by the loader tests. Regenerate with `assets/gen_triangle_gltf.py`. |
| `assets/textured_quad.glb` | Checkerboard drawn by `gltf_textured`. Regenerate with `assets/gen_textured_quad.py`. |
| `assets/marble_maze.glb` | Level for `marble_maze`. Regenerate with Blender and `assets/gen_marble_maze.py`. |

The guides are [`docs/3d-guide.md`](../docs/3d-guide.md) and [`docs/blender-to-macroquad.md`](../docs/blender-to-macroquad.md).
