# Blender to macroquad

A level is built in Blender, exported as glTF 2.0, and loaded with the `gltf` feature. The CPU loader does not open a window. Drawing the meshes does.

```toml
[dependencies]
macroquad = { version = "0.4", features = ["gltf"] }
```

On this fork the dependency is the crate in the repository root. Tests and examples that only need `Aabb` or `Sphere` leave the feature off.

## Axes

Blender is Z-up. glTF is Y-up, right-handed, and in meters. Blender's glTF exporter converts Z-up to Y-up when **+Y Up** is checked, which is the default. `GltfScene::from_bytes` stores those glTF axes as written. It does not convert them again.

`scene.with_up_axis(up)` rotates the loaded nodes, mesh placement, colliders, and spawns so glTF +Y points along `up`. `Vec3::Y` returns an equal scene. `Vec3::Z` matches `Camera3D`'s default up, which is +Z. That camera default stays +Z.

```rust
let scene = GltfScene::from_bytes(&bytes).expect("level");
let scene = scene.with_up_axis(Vec3::Z);
```

`GltfModel` parses the file on its own and stays Y-up. A camera that draws `model.draw()` uses `up: Vec3::Y` unless you exported a different orientation. Leaving **+Y Up** checked is the path the examples use.

## Mark colliders, triggers, and spawns

Empties are the usual markers (Add → Empty → Plain Axes). The loader reads each node in two ways. Node extras win. A name suffix is the fallback when `mq_role` is absent.

Put custom properties on the **object**, not on the mesh data. The exporter writes object custom properties to the glTF node `extras` only when **Custom Properties** is included.

| `mq_role` or name suffix | Result |
| --- | --- |
| `collider` or `_collider` | A solid in `GltfScene::colliders` with `trigger == false` |
| `trigger` or `_trigger` | The same shapes, with `trigger == true` |
| `spawn` or `_spawn` | A `GltfSpawn`: world position and name. `mq_shape` is ignored |

`mq_shape` is `box`, `sphere`, or `capsule`.

- An empty `box` is centered on the world-matrix translation. The full size is the length of each column of the world matrix's upper 3×3. Parent scale is included. A negative scale stays a positive length. When the world rotation leaves the faces on the world axes, including a quarter turn, the shape is an `Aabb` and the size sits on those world axes (a 90° yaw swaps local X and Z). Any other rotation is an `Obb` from `Obb::from_world_matrix`: `half_extents` are half of that full size, and a negative scale stays a positive extent.
- A mesh with `mq_shape` = `box` uses the **mesh's** local bounds, posed by the same world matrix, not the node scale alone. A default Blender cube is size 2 (vertices at ±1). With node scale 1 that box has full size 2. A cube created at size 1 (vertices at ±0.5) keeps the node scale as its full size. A rotated mesh box is an `Obb` of those bounds. A mesh with no `mq_shape` is still the axis-aligned bound of the transformed positions, which is larger than a rotated cube.
- A `sphere` uses that same center. The radius is the **largest** of the three column lengths, not half of it. An empty scaled `(2, 2, 2)` has radius `2`. The mesh is ignored when `mq_shape` is `sphere`.
- A `capsule` is a `Capsule` along the node's world +Y (the Y column of the world matrix, so a rotated empty leans). The radius is the larger of the world X and Z column lengths. The world Y column length is the distance from the center to an endpoint of the **inner** segment. The rounded caps extend `radius` past those endpoints. An empty scaled `(0.5, 1.0, 0.5)` has radius `0.5` and inner endpoints 1.0 above and below the center. The mesh is ignored when `mq_shape` is `capsule`.
- A collider or trigger with no `mq_shape` and with a mesh becomes an `Aabb` around the mesh positions after the node world matrix.
- An empty with a role, no `mq_shape`, and no mesh is not a collider.

`GltfColliderShape::overlaps_sphere` and `overlaps_aabb` cover `Aabb`, `Sphere`, `Obb`, and `Capsule`.

`GltfCollider::node` is an index into `GltfScene::nodes`, which is preorder of the default scene. It is not the index in the glTF file when some nodes were left out of the scene.

Name suffixes are case-sensitive. `Crate_collider` does not match. Extras override a suffix: a node named `pad_collider` with `mq_role` set to `spawn` is a spawn, not a collider.

## Export from Blender

File → Export → glTF 2.0 (`.glb` / `.gltf`).

1. **Format: glTF Binary (`.glb`)** when the game will draw the level. `GltfModel` only accepts a `.glb`. `GltfScene::from_bytes` also accepts a `.gltf` whose buffers are `data:` URIs. External `.bin` files and external image files are errors.
2. **Include → Custom Properties**, so `mq_shape` and `mq_role` reach the node extras. Name suffixes still work if this box is unchecked.
3. **Transform → +Y Up** stays checked.
4. **Data → Mesh**: apply modifiers if the collider should see the modified surface, and export UVs and normals if you will draw the mesh.
5. **Data → Material**: export materials, and keep images packed into the `.glb` (the exporter's automatic image mode). PNG, JPEG, and WebP are decoded. A WebP image uses `EXT_texture_webp`.
6. Leave Draco compression off. This loader does not decode it.
7. Do not export a camera or a light unless you want those nodes in the hierarchy. They are not special-cased.

Custom property names are exactly `mq_shape` and `mq_role`, string values `box` / `sphere` / `capsule` and `collider` / `trigger` / `spawn`.

A minimal room:

1. Add a mesh for the floor and the walls. These are drawn. Give them no marker suffix unless a mesh should also be a collider.
2. Add an empty named `floor_collider`, scale it to the floor's full size, and set `mq_shape` = `box`, `mq_role` = `collider`. Remember that scale is the full size, matching `draw_cube`, not a half extent.
3. Add an empty named `goal_trigger` with `mq_shape` = `sphere` and `mq_role` = `trigger`.
4. Add an empty named `player_spawn` (the suffix is enough; `mq_role` = `spawn` also works).
5. Export `room.glb` with the settings above.

## Load the level

Parsing bytes does not need a window. `include_bytes!` is the path unit tests use.

```rust
use macroquad::gltf::GltfScene;
use macroquad::math::Sphere;

let scene = GltfScene::from_bytes(include_bytes!("scene.glb")).expect("level");

let spawn = scene
    .spawns
    .iter()
    .find(|spawn| spawn.name.as_deref() == Some("player_spawn"))
    .expect("spawn");
let body = Sphere::new(spawn.position, 0.5);

for collider in &scene.colliders {
    if !collider.trigger && collider.shape.overlaps_sphere(&body) {
        // `body` starts inside this solid.
    }
}
```

Inside a running game, `load_file` reads from disk (or from the network on the web) after the window exists:

```rust
use macroquad::prelude::*;

#[macroquad::main("Level")]
async fn main() {
    let bytes = load_file("assets/room.glb").await.expect("file");
    let scene = GltfScene::from_bytes(&bytes).expect("parse");
    let model = GltfModel::from_bytes(&bytes).expect("draw");

    let start = scene
        .spawns
        .first()
        .map(|spawn| spawn.position)
        .unwrap_or(Vec3::ZERO);
    let mut player = Aabb::from_center_size(start, vec3(0.6, 1.0, 0.6));

    loop {
        clear_background(DARKGRAY);
        set_camera(&Camera3D {
            position: player.center() + vec3(0.0, 2.0, 4.0),
            target: player.center(),
            up: Vec3::Y,
            ..Default::default()
        });
        model.draw();
        draw_cube_wires(player.center(), player.size(), YELLOW);
        set_default_camera();
        next_frame().await;
    }
}
```

`GltfScene::from_bytes` and `GltfModel::from_bytes` each parse the buffer. Call both when you need markers and a draw. `GltfModel` has no method that reuses an existing `GltfScene`.

`model.draw()` submits every chunk with the current camera and the default material. The base-color factor is already in the vertex colors, at 8-bit precision. The exact factor, alpha mode, cutoff, double-sided flag, emissive factor, metallic, roughness, transmission, and unlit flag are on `part.surface`. The built-in draw does not light, sort transparency, skin, animate, or refract. Iterate `model.parts` when you need that.

`GltfModel::from_bytes_with_limits` splits a mesh into 16-bit chunks. The default limit matches `Conf`'s default draw-call capacities (10_000 vertices, 5_000 indices). A larger chunk needs both a higher `GltfMeshLimits` and higher `draw_call_vertex_capacity` / `draw_call_index_capacity`.

Node `world` is the glTF hierarchy: local translation, rotation, and scale composed with the parent. Mesh positions stay in mesh-local space on `GltfScene`. `GltfModel` has already multiplied them by `world`.

## Samples in this repository

These files are generated by Python so a build does not need Blender. The markup is the same markup an exported Blender empty produces.

| File | What it is |
| --- | --- |
| `examples/assets/triangle.gltf` and `triangle.glb` | A small hierarchy, one box collider (`crate_collider`, extras), and one spawn (`player_spawn`, name suffix). No images. |
| `examples/assets/textured_quad.glb` | A 4×4 checkerboard quad used by `examples/gltf_textured.rs`. |
| `examples/assets/gen_triangle_gltf.py` | Regenerates the triangle pair. |
| `examples/assets/gen_textured_quad.py` | Regenerates the checkerboard. |

```sh
cargo run --features gltf --example gltf_textured
cargo run --features gltf --example gltf_textured -- path/to/room.glb
```

With no path, the example shows the checkerboard. A path is loaded with `load_file`.

## What the loader does not do yet

- `GltfModel` is not reoriented by `GltfScene::with_up_axis`. Draw and markers stay in agreement when the camera uses the same up axis as the file, which is Y for a normal Blender export.
- Images are not stored on `GltfScene`. Only `GltfModel` uploads them, and only from a `.glb`.
- Skins, animations, morph targets, and PBR shading are not implemented.
- A full room asset is not checked in yet. The triangle and checkerboard files are the fixtures the tests and the draw example use.
