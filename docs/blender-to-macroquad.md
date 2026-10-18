# Blender to macroquad

A level is built in Blender, exported as glTF 2.0, and loaded with the `gltf` feature. The CPU loader does not open a window. Drawing the meshes does.

```toml
[dependencies]
macroquad = { path = "../macroquad", features = ["gltf"] }
```

Set `path` to this fork's checkout relative to your game's `Cargo.toml`; the
example assumes sibling game and macroquad directories. These APIs belong to
this fork, so a crates.io version alone does not select them. Tests and examples
that only need `Aabb` or `Sphere` leave the feature off.

## Axes

Blender is Z-up. glTF is Y-up, right-handed, and in meters. Blender's glTF exporter converts Z-up to Y-up when **+Y Up** is checked, which is the default. `GltfScene::from_bytes` stores those glTF axes as written. It does not convert them again.

`scene.with_up_axis(up)` rotates the loaded nodes, mesh placement, colliders, and spawns so glTF +Y points along `up`. `model.with_up_axis(up)` rotates the baked draw positions, normals, and bounds by that same rotation. `Vec3::Y` returns an equal scene and a model with the same vertices. `Vec3::Z` matches `Camera3D`'s default up, which is +Z. That camera default stays +Z. The two loaders do not share a value: call both helpers with the same axis, or neither.

```rust,no_run
use macroquad::prelude::*;
# fn reorient(bytes: &[u8]) {
let scene = GltfScene::from_bytes(&bytes).expect("level").with_up_axis(Vec3::Z);
let model = GltfModel::from_bytes(&bytes).expect("draw").with_up_axis(Vec3::Z);
# }
```

`GltfModel::from_bytes` still stores glTF Y-up until `with_up_axis`. A camera that draws an unreoriented model uses `up: Vec3::Y`. Leaving **+Y Up** checked is the path the examples use.

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
- A mesh with `mq_shape` = `box` uses the **mesh's** local bounds, posed by the same world matrix, not the node scale alone. A default Blender cube is size 2 (vertices at ±1). With node scale 1 that box has full size 2. A cube created at size 1 (vertices at ±0.5) keeps the node scale as its full size. A rotated mesh box is an `Obb` of those bounds. A parent scaled on one axis with a rotated child shears the world matrix; that mesh box keeps the decomposed rotation and grows until the transformed bounds are inside it. An empty box does not grow. A mesh with no `mq_shape` is still the axis-aligned bound of the transformed positions, which is larger than a rotated cube.
- A `sphere` uses that same center. The radius is the **largest** of the three column lengths, not half of it. An empty scaled `(2, 2, 2)` has radius `2`. The mesh is ignored when `mq_shape` is `sphere`.
- A `capsule` is a `Capsule` along the node's world +Y (the Y column of the world matrix, so a rotated empty leans). The radius is the larger of the world X and Z column lengths. The world Y column length is the distance from the center to an endpoint of the **inner** segment. The rounded caps extend `radius` past those endpoints. An empty scaled `(0.5, 1.0, 0.5)` has radius `0.5` and inner endpoints 1.0 above and below the center. The mesh is ignored when `mq_shape` is `capsule`.
- A collider or trigger with no `mq_shape` and with a mesh becomes an `Aabb` around the mesh positions after the node world matrix.
- An empty with a role, no `mq_shape`, and no mesh is not a collider.

`GltfColliderShape::overlaps_sphere` and `overlaps_aabb` cover `Aabb`, `Sphere`, `Obb`, and `Capsule`.

`GltfCollider::node` is an index into `GltfScene::nodes`, which is preorder of the default scene. It is not the index in the glTF file when some nodes were left out of the scene.

The suffix compare is case-sensitive and must be the exact ending `_collider`, `_trigger`, or `_spawn`. `Crate_collider` matches `_collider`. `Player_Spawn` does not match `_spawn`. A matching empty still needs `mq_shape`, or a mesh, before it is emitted as a collider. Extras override a suffix: a node named `pad_collider` with `mq_role` set to `spawn` is a spawn, not a collider.

Blender duplicates commonly receive a suffix such as `.001`:
`player_spawn.001` no longer ends in `_spawn`. Set `mq_role = spawn` on the
object to make that duplicate a spawn. Arbitrary properties such as `mq_tag`
are not exposed by the current scene API. `GltfSpawn` has `node`, `name`, and
`position`; get its full pose from `scene.nodes[spawn.node].world` and choose
the player's collision radius in the game.

## 1. Build a small level

Start with a new Blender scene and remove its default objects. Use Blender's
Z-up coordinates for the following recipe; export converts them to Y-up.

1. Add four cubes and enter these **locations and dimensions** in the Item
   panel. Give each cube object string custom properties `mq_role = collider`
   and `mq_shape = box`. The same mesh will be visible and solid. Mesh boxes
   use their vertex bounds, so there is no need to compensate for Blender's
   default cube size.

   | Object | Location (X, Y, Z) | Dimensions (X, Y, Z) | Z rotation |
   | --- | --- | --- | --- |
   | `floor_collider` | `(0, 0, -0.1)` | `(8, 8, 0.2)` | `0°` |
   | `left_collider` | `(-4, 0, 1)` | `(0.2, 8, 2)` | `0°` |
   | `right_collider` | `(4, 0, 1)` | `(0.2, 8, 2)` | `0°` |
   | `cover_collider` | `(0, 1.5, 0.5)` | `(2, 0.4, 1)` | `35°` |

   Set the cover's dimensions before rotating it. Its explicit box markup
   preserves an oriented collider instead of the larger world AABB used for a
   mesh with only a role suffix.
2. Add a Plain Axes empty at `(0, -2, 0.4)`, name it `goal_trigger`, set its
   **scale** to `(0.6, 0.6, 0.6)`, and add `mq_role = trigger`,
   `mq_shape = sphere`. The resulting trigger has radius `0.6`.
3. Add a Plain Axes empty at `(0, 0, 0.4)`, name it `player_spawn`, and add
   `mq_role = spawn`. This tutorial interprets its position as the center of
   a sphere of radius `0.4`, resting on the floor. The file stores no body size.
4. Give the cubes simple opaque base-color materials. Empties have no draw
   geometry; the game below outlines the goal sphere itself.

## 2. Export from Blender

File → Export → glTF 2.0 (`.glb` / `.gltf`).

1. **Format: glTF Binary (`.glb`)** when the game will draw the level. `GltfModel` only accepts a `.glb`. `GltfScene::from_bytes` also accepts a `.gltf` whose buffers are `data:` URIs. External `.bin` files are unsupported; an external image is an error when a loaded primitive samples it.
2. **Include → Custom Properties**, so `mq_shape` and `mq_role` reach the node extras. Name suffixes still work if this box is unchecked.
3. **Transform → +Y Up** stays checked.
4. **Data → Mesh**: apply modifiers if the collider should see the modified surface, and export UVs and normals if you will draw the mesh.
5. **Data → Material**: export materials, and keep images packed into the `.glb` (the exporter's automatic image mode). PNG, JPEG, and WebP are decoded. A WebP image uses `EXT_texture_webp`.
6. Leave Draco compression off. This loader does not decode it.
7. Do not export a camera or a light unless you want those nodes in the hierarchy. They are not special-cased.

Custom property names are exactly `mq_shape` and `mq_role`, string values `box` / `sphere` / `capsule` and `collider` / `trigger` / `spawn`.

Export the file to `assets/room.glb` in your game project. Ensure the marker
empties are included if exporting only selected objects. Keep the `.blend` as
the editable source. No Blender installation is required when running the game.

## 3. Inspect the CPU scene

Parsing bytes does not need a window. For a file embedded at compile time, use
`include_bytes!("../assets/room.glb")` from `src/main.rs`. The following snippet
accepts the file's bytes so it also works with another loading mechanism.

```rust
use macroquad::gltf::GltfScene;
use macroquad::math::Sphere;
# fn inspect(bytes: &[u8]) {
let scene = GltfScene::from_bytes(bytes).expect("level");

let spawn = scene
    .spawns
    .iter()
    .find(|spawn| spawn.name.as_deref() == Some("player_spawn"))
    .expect("spawn");
let body = Sphere::new(spawn.position, 0.4);

for collider in &scene.colliders {
    if !collider.trigger && collider.shape.overlaps_sphere(&body) {
        // `body` touches or overlaps this solid; boundaries are inclusive.
    }
}
# }
```

CPU parsing needs no graphics context and is suitable for tests. The floor is
expected to overlap the resting player's sphere because touching counts.

## 4. Upload once, move, and draw

Put the following in `src/main.rs` and run `cargo run` from the game project.
`load_file` reads from disk (or from the network on the web). The GPU model is
created inside a running macroquad application. This sample moves horizontally
at a fixed height to focus on slide queries; it has no gravity or jumping.

```rust,no_run
use macroquad::prelude::*;
use macroquad::gltf::GltfColliderShape;

#[macroquad::main("Level")]
async fn main() {
    let bytes = load_file("assets/room.glb").await.expect("file");
    let scene = GltfScene::from_bytes(&bytes).expect("parse");
    let cpu = GltfModel::from_bytes(&bytes).expect("draw model");
    // false releases this model's CPU vertices and indices after upload.
    let model = RetainedGltfModel::from_model(cpu, false).expect("upload");

    let mut center = scene
        .spawns
        .iter()
        .find(|spawn| spawn.name.as_deref() == Some("player_spawn"))
        .map(|spawn| spawn.position)
        .expect("player_spawn");
    let (mut aabbs, mut obbs, mut spheres, mut capsules) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for collider in scene.colliders.iter().filter(|c| !c.trigger) {
        match collider.shape {
            GltfColliderShape::Aabb(shape) => aabbs.push(shape),
            GltfColliderShape::Obb(shape) => obbs.push(shape),
            GltfColliderShape::Sphere(shape) => spheres.push(shape),
            GltfColliderShape::Capsule(shape) => capsules.push(shape),
        }
    }
    // Markers are all we need after extracting the solids and spawn. Drop the
    // separate CPU scene's mesh/image data as well as the input file bytes.
    let triggers: Vec<_> = scene.colliders.iter().filter(|c| c.trigger).cloned().collect();
    drop(scene);
    drop(bytes);
    let radius = 0.4;

    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        let mut wish = Vec3::ZERO;
        if is_key_down(KeyCode::A) { wish.x -= 1.0; }
        if is_key_down(KeyCode::D) { wish.x += 1.0; }
        if is_key_down(KeyCode::W) { wish.z -= 1.0; }
        if is_key_down(KeyCode::S) { wish.z += 1.0; }
        let delta = wish.normalize_or_zero() * 3.0 * get_frame_time().min(0.05);
        center = slide_sphere_solids(
            center, radius, delta, &aabbs, &obbs, &spheres, &capsules,
        );
        let body = Sphere::new(center, radius);
        let reached_goal = triggers.iter().any(|c| c.shape.overlaps_sphere(&body));

        clear_background(DARKGRAY);
        let camera = Camera3D {
            position: vec3(8.0, 8.0, 10.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            ..Default::default()
        };
        set_camera(&camera);
        if let Some(frustum) = camera.frustum() {
            model.draw_culled(Mat4::IDENTITY, &frustum);
        } else {
            model.draw(Mat4::IDENTITY);
        }
        draw_sphere(center, radius, None, YELLOW);
        for trigger in &triggers {
            if let GltfColliderShape::Sphere(goal) = trigger.shape {
                draw_sphere_wires(goal.center, goal.radius, None, GREEN);
            }
        }
        set_default_camera();
        let label = if reached_goal { "Goal reached!" } else { "WASD: move | Escape: quit" };
        draw_text(label, 20.0, 30.0, 24.0, WHITE);
        next_frame().await;
    }
}
```

`GltfScene::from_bytes` and `GltfModel::from_bytes` each parse the buffer. Call both when you need markers and a draw. `GltfModel` has no method that reuses an existing `GltfScene`. When the camera's up axis is not glTF +Y, call `with_up_axis` on each result with that same axis **before** retained upload.

`RetainedGltfModel` clones share immutable GPU geometry and textures. A clone
with `draw(transform)` places another instance of that geometry, but does not
clone or move the CPU scene's colliders. `draw_culled` tests transformed chunk
bounds and returns the number submitted. `world_bounds(transform)` supports
application sorting and queries; it uses the supplied matrix only, while
`draw_culled` also includes the current model stack. This is one ordinary draw
per submitted chunk, not hardware instancing or an automatic frame-time gain.
Use `from_model(cpu, true)` only when you need `cpu_model()` later.

The example explicitly excludes triggers from the slide slices. A goal uses
the same shape queries as a wall; including it among solids would make it
block the player. `slide_sphere_solids` does not provide grounding, stairs,
slope limits, or a contact list. See the [3D guide](3d-guide.md) for the current
contact, sweep, and slide contracts.

## CPU images and draw materials

`scene.images` holds the base-color images sampled by loaded primitives, decoded to 8-bit RGBA (`GltfImage`: `width`, `height`, and row-major `pixels` with row 0 at the top). `GltfPrimitive::base_color_texture` is an index into that list, or `None` when the material has no base-color image. Images no loaded primitive samples are omitted. Normal, metallic-roughness, occlusion, and emissive maps are not loaded. The bytes are the file's stored channels (sRGB for a base-color texture), not linear, and not a `Texture2D`. `with_up_axis` does not move them.

```rust
use macroquad::gltf::GltfScene;
# fn inspect_image(scene: &GltfScene) {
if let Some(texture) = scene.meshes[0].primitives[0].base_color_texture {
    let image = &scene.images[texture];
    let rgba = &image.pixels[0..4]; // R, G, B, A of the top-left pixel
}
# }
```

`GltfModel::draw()` and `RetainedGltfModel::draw(transform)` submit every chunk
with the current camera and **current material**. The default material is
unlit. The base-color factor is already in the vertex colors, at 8-bit
precision. The exact factor, alpha mode, cutoff, double-sided flag, emissive
factor, metallic, roughness, transmission, and unlit flag are on
`part.surface`. Built-in drawing does not apply alpha masks, choose sidedness,
light, sort transparency, skin, animate, or refract. Iterate `model.parts` and
select custom materials when you need those behaviors.

`part.emissive_texture` retains the optional embedded emissive map. In a lighting shader, decode both color images from sRGB to linear RGB, multiply the emissive image by `part.surface.emissive`, and add it to the lit base color. When there is no emissive map, use a white texture. An emissive factor is not a flat glow when the material also supplies an emissive map; ignoring the map washes out dark details. Base-color and emissive images share the mesh's one UV channel: the loader applies texture-coordinate selection and `KHR_texture_transform`, and returns an error if the two resulting mappings differ.

For glass, submit all opaque geometry first, then blended geometry back to front, using `depth_test: Comparison::LessOrEqual` and `depth_write: false`. The fork honors these independently on OpenGL/WebGL, including a workaround for miniquad 0.4.11's depth-state coupling. Setting a custom material texture also snapshots it for each queued draw, so one reusable shader can render several objects with different emissive maps. `cargo run --example material_depth_check` verifies occlusion, depth masks, depth clearing and queued samplers with GPU pixel assertions.

A retained custom vertex shader applies `Model` to positions. Use
`GltfModelTransform::new(full_transform)` for the inverse-transpose normal
matrix and mirrored flag; normalize transformed normals and reverse the
front-face pipeline choice for mirrored instances when culling. Include any
parent model matrix. The helper returns `None` when inverse normals are not
finite. The adapter sets neither these custom uniforms nor the pipeline.
Sort blended parts using the same transformed bounds used for drawing. See
the [retained geometry contract](3d-guide.md#retained-geometry).

## Loading budgets

`GltfModel::from_bytes_with_limits` splits a mesh into 16-bit chunks. The default limit matches `Conf`'s default draw-call capacities (10_000 vertices, 5_000 indices). For immediate `draw_mesh` / `GltfModel::draw`, a larger chunk needs both a higher `GltfMeshLimits` and higher `draw_call_vertex_capacity` / `draw_call_index_capacity`. Retained draws bypass streaming capacities, but their source chunks still use 16-bit indices and must be valid triangle meshes.

The draw-call capacities are upper limits, not a GPU reservation for every material group. Each streaming vertex/index buffer starts at the submitted chunk's actual size and grows geometrically only when a later draw needs more space, bounded by the configured limit. Buffers retain their high-water capacity for reuse; changing the limits releases them for lazy recreation. Clearing queued draws also discards their queued CPU geometry. This keeps large permitted chunks affordable in scenes that contain hundreds of small meshes. Final GLB mesh chunks discard unused vector capacity, and conversion releases each source mesh after its last node instance so later image decoding can reuse that CPU memory.

For a browser or a scene with many large images, opt into a texture budget:

```rust,no_run
use macroquad::gltf_draw::{GltfLoadOptions, GltfModel};
# fn load(bytes: &[u8]) -> Result<GltfModel, String> {
let model = GltfModel::from_bytes_with_options(&bytes, GltfLoadOptions {
    max_texture_size: Some(512),
    ..Default::default()
})?;
# Ok(model)
# }
```

Both image dimensions stay at or below this limit; a 4096×2048 image becomes 512×256. Smaller images stay unchanged, and the aspect ratio, UVs, and sampler settings are preserved. This applies to embedded base-color and emissive images and their mipmaps. `None` is the default, preserving the original full-resolution behavior of the existing loading methods. The cap bounds retained GPU image memory, but the decoder still briefly holds the original image in CPU memory. It does not edit or re-encode the source GLB. Supply `mesh_limits` in the same options value when changing mesh chunk sizes as well.

The cap does not resize the separate `GltfScene::images` CPU list. Retaining
that scene keeps its decoded images and local mesh arrays alive even after
`RetainedGltfModel::from_model(cpu, false)` releases draw-model geometry.
Extract the markers you need and drop the scene data when appropriate, as in
the sample. `model.stats` reports counts rather than memory estimates or
process/driver memory measurements.

Node `world` is the glTF hierarchy: local translation, rotation, and scale composed with the parent. Mesh positions stay in mesh-local space on `GltfScene`. `GltfModel` has already multiplied them by `world`.

## Samples in this repository

These files are generated by Python so a build does not need Blender. The markup is the same markup an exported Blender empty produces.

| File | What it is |
| --- | --- |
| `examples/assets/triangle.gltf` and `triangle.glb` | A small hierarchy, one box collider (`crate_collider`, extras), one spawn (`player_spawn`, name suffix), and a 1×1 base-color image (`pixel`, RGBA 10, 20, 30, 255). |
| `examples/assets/textured_quad.glb` | A 4×4 checkerboard quad used by `examples/gltf_textured.rs`. |
| `examples/assets/gen_triangle_gltf.py` | Regenerates the triangle pair. Does not need Blender. |
| `examples/assets/gen_textured_quad.py` | Regenerates the checkerboard. Does not need Blender. |

```sh
cargo run --features gltf --example gltf_textured
cargo run --features gltf --example gltf_textured -- path/to/room.glb
cargo run --features gltf --example gltf_retained
```

With no path, `gltf_textured` shows the checkerboard. A path is loaded with
`load_file`. `gltf_retained` uses that fixture for three moving, shared-buffer
instances, including a reflection. The [example index](../examples/README.md)
also covers picking, frustum culling, contacts, sweeps, and slide queries.

## What the loader does not do yet

- `GltfScene::with_up_axis` does not change a `GltfModel`, and the reverse is also true. Call both with the same axis when the draw and the markers should leave Y-up. A camera can instead keep `up: Vec3::Y` and skip both calls. That is what the examples do.
- `GltfModel` still uploads its own GPU textures, and only from a `.glb`. Sampler wrap and filter stay on that draw path. `GltfScene` does not turn its RGBA bytes into a `Texture2D`.
- Skins, animations, editable node poses, morph targets, normal/roughness maps, and automatic PBR shading are not implemented. Retained transforms move baked geometry; collider poses and material rendering remain the caller's responsibility.
- `assets/room.glb` is the file you export in this tutorial, not a checked-in asset. The triangle and checkerboard files are the small fixtures in this repository. The historical Marble Maze and Cinder Court game rounds are documented in the roadmap; their games and level assets are not distributed here.
