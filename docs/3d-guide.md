# 3D guide

This fork adds 3D queries to `macroquad::math`. They are also in the prelude (`use macroquad::prelude::*`). The same types are in `macroquad::math` for a test or a tool that must not open a window.

Nothing here assumes an up axis. `Camera3D::up` stays caller-defined. The examples in this repository use **Y-up**, which matches a glTF file exported from Blender. Sizes are unitless. A glTF scene is in meters, and `get_frame_time` is in seconds, so a gravity of `(0, -9.8, 0)` is meters per second squared when the level is left in those units.

`draw_cube` / `draw_cube_wires` take a center and a **full** size. `Aabb::from_center_size` uses that same pair. `Obb::half_extents` is half of that full size. `Capsule` `a` and `b` are the inner segment, so the hull extends `radius` past each end.

`CharacterController` provides kinematic AABB movement with sliding, ground
snapping, slope limits and opt-in stair stepping. The caller owns the body,
velocity and gravity; there is no rigid-body world. `Contact`, `SweepHit`,
`slide_sphere`, `slide_sphere_solids` and `slide_spheres_solids` remain available
for custom response.
Choose the movement path according to the result you need:

| Need | Public API | Caller responsibility |
| --- | --- | --- |
| Inspect a current overlap | Shape `contact` methods | Decide whether and how to move the shapes |
| Constrain one requested translation against static solids | Sphere slide helpers, including `slide_spheres_solids` for a fixed hull | Own velocity, gravity and orientation; apply the returned position or translation as documented |
| Move an AABB character with ground handling | `CharacterController` | Supply velocity, elapsed seconds, solids and optional step height |
| Integrate a mass and exchange a normal impulse | `RigidBody::integrate`, then explicit pair response | Choose candidate pairs, response order and simulation time steps |

`UniformGrid` can supply mutable candidate snapshots for these paths;
[Octree](#static-octree-candidates) provides immutable snapshots for static bounds.
Both store caller indices and neither moves bodies nor dispatches contacts.
[TriangleMesh](#triangle-mesh-surfaces) and [Heightfield](#heightfield-terrain)
query surfaces, without adding automatic body or controller response.
[DistanceJoint](#distance-joints) constrains body origins after integration;
the caller chooses pair order and the number of corrections per step.
The step-by-step [Blender tutorial](blender-to-macroquad.md) covers exporting,
loading, drawing and moving through a marked level. The
[example index](../examples/README.md) lists runnable samples.

Imported levels continue in [Retained geometry](#retained-geometry), [Resource diagnostics](#resource-diagnostics), [Retiring unused render resources](#retiring-unused-render-resources), [Pollable glTF construction](#pollable-gltf-construction), [Loading and materials](#loading-and-materials), and [Opt-in glTF material passes](#opt-in-gltf-material-passes). That last section includes selected-part submission, [optional basic lighting](#optional-basic-lighting), and shaded normal/roughness maps. [Replacing an imported node pose](#replacing-an-imported-node-pose), [Editing an imported hierarchy](#editing-an-imported-hierarchy), [CPU morph data and evaluation](#cpu-morph-data-and-evaluation), and [Drawing independent morph instances](#drawing-independent-morph-instances) cover moving a baked node and drawing shape keys. Collision queries start at [Ray hits](#ray-hits). Caller-owned gravity and supported AABB movement are in [Physics](#physics) and [Character movement](#character-movement). Mouse look uses [Logical and physical input](#logical-and-physical-input) and [Pointer capture](#pointer-capture). Browser density and frame-rate caps are in [Browser canvas budgets](#browser-canvas-budgets).

For animated levels, continue with [Playing glTF clips](#playing-gltf-clips)
and the [per-frame update order](#update-order-for-an-animated-level). The
[UI and 2D guide](ui-2d-guide.md) covers the existing menu, camera and sprite
building blocks alongside these 3D paths. For a combined frame, see
[Draw a HUD over a level](#draw-a-hud-over-a-level).

## Retained geometry

`Mesh` and `draw_mesh` remain the immediate path. For unchanged geometry,
`RetainedMesh::from_mesh(&mesh)?` uploads immutable vertex/index buffers once.
`draw_retained_mesh(&retained, transform)` queues a draw using the current
camera, material, viewport, scissor and render target. Clones share the buffers;
each submission remains an ordinary draw call, without hardware instancing.

```rust,no_run
use macroquad::prelude::*;
# fn draw_instances(mesh: &Mesh) -> Result<(), RetainedMeshError> {
let retained = RetainedMesh::from_mesh(mesh)?; // once, outside the frame loop
draw_retained_mesh(&retained, Mat4::IDENTITY);
draw_retained_mesh(&retained, Mat4::from_translation(vec3(2.0, 0.0, 0.0)));
# Ok(()) }
```

Immediate and retained submissions keep their relative order, and custom
uniforms and texture bindings are captured per draw. The supplied transform is
multiplied after the model stack. Custom shaders receive that composed `Model`
matrix; they still handle normal transforms and reflected winding/culling.
The handle's `texture` may be changed without reuploading geometry.

Metal custom shaders receive built-in uniform bytes in the order `Projection`,
`Model`, `_Time`, followed by the declared custom uniforms. Match that order in
the MSL buffer struct and compute `Projection * Model * position`, as in
`examples/custom_material.rs`. GLSL uniforms are bound by name.

The source CPU mesh can be released after upload. Invalid indices, incomplete
triangles, empty meshes and meshes exceeding the `u16` vertex range are rejected
before allocation. Streaming batch capacity does not clamp retained geometry.
Queued draws keep their mesh alive even if the caller drops its last handle.
Unused buffers are collected on a renderer flush or the next retained upload;
ordinary frame completion flushes automatically. Cancelled draws release their
queued references too. Live retained buffers survive streaming-capacity resets.
See `examples/retained_mesh.rs` for a complete example, and resource diagnostics
below for allocation estimates and upload counters.

With `--features gltf`, `RetainedGltfModel::from_model(model, false)` consumes
an existing `GltfModel`, uploads each chunk once and releases its CPU vertices
and indices. Pass `true` to keep the original model available through
`cpu_model()`. Clones share buffers, textures and any retained CPU model.
Finish CPU geometry edits and `with_up_axis` before this conversion.

```rust,no_run
use macroquad::gltf_draw::{GltfModel, RetainedGltfModel};
use macroquad::prelude::*;
# fn example(model: GltfModel, camera: &Camera3D) -> Result<(), RetainedMeshError> {
let retained = RetainedGltfModel::from_model(model, false)?;
let instance = Mat4::from_translation(vec3(2.0, 0.0, 0.0));
retained.draw(instance);
// For culling, replace the draw above with:
// if let Some(frustum) = camera.frustum() {
//     retained.draw_culled(instance, &frustum);
// }
# Ok(()) }
```

The adapter keeps the current camera and material. Its `parts` retain material,
node and bounds metadata for application lighting, filtering and alpha sorting.
`part.draw(transform)` submits one chunk; `part.world_bounds(transform)` encloses
all transformed corners, including nonuniform scale, reflection and shear.
`draw_culled` also includes the current low-level model stack and returns the
number of submitted chunks. `world_bounds` uses only the supplied transform;
include a parent model matrix yourself when using those bounds for sorting or
queries. Culling skips parts with invalid bounds or transforms. Non-finite or
projective transforms are not supported. An empty model has no world bounds.

These transforms are relative to the already baked scene geometry. Rotating
one chunk around its imported bounds center uses a pivot transform such as
`Mat4::from_translation(new_center) * Mat4::from_quat(rotation) *
Mat4::from_translation(-part.center)`. For an object split across chunks, use
one shared pivot for all its parts. The pose sidecar described below retains
authored pivots and editable hierarchy state for this purpose. Keep gameplay
colliders consistent with the transform you submit; they are not updated automatically.

Custom vertex shaders apply `Model` to positions. For lighting, obtain the
inverse-transpose `normal_matrix` from `GltfModelTransform::new(transform)` and
normalize transformed normals. The helper returns `None` for transforms that
cannot supply finite inverse normals. A mirrored transform reverses triangle
winding: when culling, select the opposite `FrontFaceOrder` pipeline when the
helper's `mirrored` flag is true. The adapter does not change a custom material's
pipeline or uniforms. Include any parent model stack in the helper's transform.
The default shader is unlit and does not cull. These are shared geometry draws,
not hardware instancing; no frame-time improvement is implied by buffer reuse.
See `examples/gltf_retained.rs`.

## Resource diagnostics

Resource reports are explicit snapshots; they do not change rendering or
collect unused resources. `GltfModelStats` keeps its original loader counts.
With `gltf`, call `model.resource_report()` or combine several CPU and retained
models with `GltfResourceReport::collect`. A combined report counts shared CPU
allocations, retained buffers and textures once, including resources referenced
by an optional retained CPU model. Separate reports must not be summed when
their models share resources.

```rust,no_run
use macroquad::gltf_draw::{
    set_gltf_resource_diagnostics_enabled, GltfModel, GltfResourceReport,
    RetainedGltfModel,
};
# fn example(bytes: &[u8]) -> Result<(), String> {
// Opt in before loading to preserve source image identifiers and dimensions.
set_gltf_resource_diagnostics_enabled(true);
let cpu = GltfModel::from_bytes(bytes)?;
let retained = RetainedGltfModel::from_model(cpu, false).map_err(|e| e.to_string())?;
let instance = retained.clone();
let report = GltfResourceReport::collect(&[], &[&retained, &instance]);
println!("{report:#?}");
# Ok(()) }
```

Source-image tracking is disabled by default and stores only small metadata
with weak texture references. Enabling it after loading cannot reconstruct
earlier source dimensions; disabling clears the metadata. Uploaded dimensions
and texture estimates remain available without source tracking. A capped
texture's source dimensions refer to the input GLB, before upload resizing;
offline-prepared GLBs naturally report their prepared dimensions. Textures
replaced by application code have no glTF source metadata.

`macroquad::telemetry::renderer_diagnostics()` reports current CPU batch lengths
and capacities, renderer-owned retained buffers, streaming GPU buffer capacity,
queued draws and optional cumulative counters. Enable counters with
`set_renderer_diagnostics_enabled(true)` and reset the measurement interval with
`reset_renderer_diagnostics()`. Read after `next_frame().await` to include that
frame's flushed draws, then reset before the next interval. A snapshot taken
before a flush distinguishes queued work from backend draws already submitted.
Enabling counters does not reconstruct earlier uploads.

CPU byte counts describe vertex/index elements and their allocation capacities,
excluding model metadata and allocator overhead. GPU buffer estimates use
buffer payload/capacity; texture estimates sum allocated pixel levels including
mipmaps. The model report covers its referenced base-color/emissive textures,
not all application textures. The renderer report covers geometry storage and
uploads; it does not include framebuffer, shader, video, font or driver memory.
Raw backend changes can bypass texture metadata. None of these estimates is
total process memory, physical GPU residency, a decoder peak, or a frame-time
benchmark. Model and renderer buffer totals overlap: do not add them together.

Dropped handles can remain in the renderer report until a flush collects them;
pending draws deliberately keep their resources alive. Compare snapshots at
the same frame boundary when testing repeated load/drop cycles.

## Retiring unused render resources

At a scene transition, use `macroquad::window::retire_unused_render_resources()`
after finishing the outgoing frame and dropping its resources, before loading
the replacement scene. Select a surviving camera and material first; release
obsolete models, textures, render targets and custom material texture bindings.
For example, a scene drawn with the default material can be released with:

```rust,no_run
use macroquad::gltf_draw::RetainedGltfModel;
use macroquad::prelude::*;
# async fn unload_scene(scene: RetainedGltfModel) {
next_frame().await;
set_default_camera();
gl_use_default_material();
drop(scene);
retire_unused_render_resources();
// Load the replacement scene here.
# }
```

The operation flushes any pending draws before collecting unused managed
textures and retained meshes. A queued draw still renders even if its last
application mesh or texture handle was dropped. Custom materials used by queued
draws must stay alive through retirement, even after switching materials.
Deferred UI commands are processed at `next_frame().await`; retirement does not
finish or clear those commands. Finish the outgoing UI frame before dropping
its resources.
Live handles and shared model instances remain
valid; resources still owned by a camera, material binding or another handle
are not unused. Ordinary pause does not retire live scene resources.

Retirement also releases cached streaming CPU/GPU buffers and draw slots while
preserving configured batch-capacity limits. Later immediate draws allocate
these caches again, so use this at transitions rather than every frame. The
operation releases renderer-owned allocations; it does not promise a reduction
in total process memory or immediate reclamation by the graphics driver. Take
resource-diagnostic snapshots at this same boundary to compare load/drop cycles.

## Pollable glTF construction

With `gltf`, `GltfModelLoader::new(bytes, settings)` creates an optional loading
handle. Its `poll()` method advances one unit, and `progress()` reports
`GltfLoadProgress { stage, completed, total }`. The counts belong to the current
stage, so do not treat them as an overall percentage. Parsing, source-image
preflight, texture loading and mesh conversion are separate stages. Each
texture upload and each node/primitive conversion gets its own poll; stage
boundaries also return to the caller. Read file bytes with `load_file` first;
the handle borrows those bytes until dropped and performs no file I/O.

After `take_model()` returns the completed `GltfModel`, pass it to
`RetainedGltfModelLoader::new(model, false)` for incremental retained conversion.
It validates every chunk before uploading any buffers, then uploads at most
one chunk per poll. The existing parsed and baked geometry and GPU textures are
reused. `false` releases CPU geometry after conversion; `true` keeps it through
the result's `cpu_model()`. This does not provide a shared parse with the
separate `GltfScene` constructor.

```rust,no_run
use macroquad::prelude::*;
# async fn load_scene(bytes: &[u8]) -> Result<RetainedGltfModel, String> {
let settings = GltfLoadOptions::default().with_image_limits(GltfImageLimits {
    max_source_pixels: Some(2048 * 2048),
    max_decoded_bytes: Some(16 * 1024 * 1024),
});
let mut source = GltfModelLoader::new(bytes, settings)?;
let cpu = loop {
    if is_key_pressed(KeyCode::Escape) {
        source.cancel();
        return Err("Loading cancelled".into());
    }
    let progress = source.poll()?;
    clear_background(BLACK);
    draw_text(format!("{:?}: {} / {}", progress.stage, progress.completed, progress.total), 20., 30., 22., WHITE);
    if let Some(model) = source.take_model() { break model; }
    next_frame().await;
};
drop(source); // Releases the source-byte borrow and remaining parse metadata.
let mut upload = RetainedGltfModelLoader::new(cpu, false);
loop {
    if is_key_pressed(KeyCode::Escape) {
        upload.cancel();
        return Err("Loading cancelled".into());
    }
    let progress = upload.poll()?;
    clear_background(BLACK);
    draw_text(format!("{:?}: {} / {}", progress.stage, progress.completed, progress.total), 20., 30., 22., WHITE);
    if let Some(model) = upload.take_model() { return Ok(model); }
    next_frame().await;
}
# }
```

Poll both handles on the application's render thread. This is cooperative
loading: the application must call `next_frame().await` to advance its loading
screen. Parsing, one primitive conversion, or one synchronous image decode
can still stall a frame; there is no strict time budget and no worker decoding.
Image limits reject oversized source dimensions before decoding but do not
bound decoder scratch memory. A large primitive can produce several chunks
during its single conversion unit. All consumed textures are decoded and
uploaded before primitive conversion, so source mesh arrays remain alive
during image decoding and are released after their last instance is converted.

`cancel()` drops partial work and an untaken completed result. Dropping a handle
has the same ownership cleanup; errors release partial work while leaving the
handle in `Failed`. Managed textures and buffers are collected by the usual
frame/renderer boundary, or by `retire_unused_render_resources()` at a suitable
transition. Already taken results remain owned by the caller. Completion is
stable across further polls, `take_model()` succeeds only once, and a cancelled
or failed handle cannot resume. Create a new handle to retry.

The synchronous `GltfModel::from_bytes*` and `RetainedGltfModel::from_model`
constructors remain available. See `examples/gltf_pollable.rs` for loading,
single-unit stepping, pause, cancellation and restart with a moving screen.

## Loading and materials

The optional `gltf` feature provides two loading paths. `GltfScene::from_bytes`
parses hierarchy, mesh data, colliders, triggers, spawns, and sampled base-color
images without opening a window. `GltfModel::from_bytes` requires a running
macroquad application and uploads textures while producing drawable chunks.
The two calls parse independently. Their `with_up_axis` helpers must use the
same axis before converting the draw model to retained geometry.

Marker roles prefer recognized `mq_role` extras, then case-sensitive name
endings `_collider`, `_trigger`, and `_spawn`. One optional `.<digits>` Blender
duplicate tail is accepted (ASCII digits), so `patrol_spawn.001` is a spawn;
`patrol_spawn.001.002` and `Patrol_Spawn.001` are not suffix matches. The full
name is retained. To select markers independently of Blender object names,
set a string `mq_tag` custom property and enable `export_extras`:

```rust
use macroquad::gltf::GltfScene;

let scene = GltfScene::from_bytes(
    br#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[
        {"name":"patrol_spawn.001","extras":{"mq_tag":"guard"}}
    ]}"#,
).unwrap();
let guard = scene.spawns.iter()
    .find(|spawn| scene.marker_tag(spawn.node) == Some("guard"))
    .unwrap();
assert_eq!(guard.name.as_deref(), Some("patrol_spawn.001"));
```

`marker_tag` reads `scene.marker_tags`, aligned with the loaded `scene.nodes`
array. Missing/non-string/nested tags and missing node indices return `None`;
empty strings, whitespace, case and Unicode are preserved. Tags are available
for all loaded nodes and do not create a role or alter a shape. Clone and
up-axis conversion preserve them. `GltfSpawn` and `GltfCollider` retain their
existing fields. An unpaired UTF-16 JSON escape cannot become a tag and returns
`None`; undecodable unrelated extras keys or values do not hide later fields.
Complete `GltfScene` literals written before tag support need a `marker_tags`
entry per node (use `None` for untagged nodes). Loader calls populate that field
automatically; keep it aligned if you reorder `nodes` yourself.

`scene.solid_colliders()` and `scene.trigger_colliders()` lazily borrow the
existing collider references by their current `trigger` flag without allocating.
They preserve stored order, names, shapes and scene-local `collider.node` indices,
including tagged Blender duplicate names. Enumerating a filtered iterator gives
filtered positions; enumerate `scene.colliders` before filtering when you need
original collider-list indices for a broadphase. These helpers do not transform
shapes, apply response or create events. The [room example](../examples/gltf_room.rs)
extracts solids for its movement loop and queries its goal separately; the
[Blender movement recipe](blender-to-macroquad.md#4-upload-once-move-and-draw)
shows all four shape slices and a separate trigger overlap query.

For a spawn or another marker, read its world orientation with
`scene.marker_rotation(node)`. The index is the scene-local `GltfNode` index,
such as `spawn.node`; it is not necessarily the node index in the source file.
The returned unit quaternion includes parent rotation and any `with_up_axis`
conversion applied to that scene:

```rust
use macroquad::gltf::GltfScene;
use macroquad::math::Vec3;
# fn spawn_forward(scene: &GltfScene) -> Option<Vec3> {
let spawn = scene.spawns.iter()
    .find(|spawn| spawn.name.as_deref() == Some("player_spawn"))?;
let rotation = scene.marker_rotation(spawn.node)?;
// This game chooses local -Z as its forward direction.
let forward = rotation * -Vec3::Z;
# Some(forward) }
```

`GltfSpawn` continues to store only `node`, `name`, and `position`. The helper
returns `None` for a missing node or a world transform from which it cannot
obtain a finite rotation. It decomposes the matrix stored in `scene.nodes`;
read that full world matrix when scale or shear also matters. Editing a separate
`GltfHierarchyPose` does not update the scene's stored marker orientation.

`GltfScene::images` contains top-row-first, 8-bit RGBA `GltfImage` values.
`GltfPrimitive::base_color_texture` indexes this decoded-image list, not the
file's texture array. Primitives sharing a source image share its entry. Only
base-color images sampled by loaded primitives are included; emissive, normal,
metallic-roughness, and occlusion maps are absent from this CPU list. Base-color
RGB remains in its stored sRGB encoding. No `Texture2D` is created on this path.

The draw loader also retains embedded emissive textures. Its `GltfSurface`
metadata includes the exact linear base-color and emissive factors, alpha mode
and cutoff, double-sided and unlit flags, metallic/roughness, and transmission.
Base-color factors are already baked into vertex colors at byte precision.
`GltfModel::draw`, `RetainedGltfModel::draw`, and individual retained part draws
submit using the **current material**. With the default material they are unlit;
none automatically implements glTF alpha masks, lighting, transparency sorting,
double-sided selection, or refraction.

For a custom renderer, decode base-color and emissive RGB to linear space and
avoid multiplying the base-color factor twice. Bind each part's optional
emissive image (white when absent) and multiply it by `surface.emissive`.
Implement `Mask` with the alpha cutoff; draw opaque/masked parts before blended
parts, then sort blended parts back to front using their transformed bounds.
Blended parts generally need depth testing with depth writes disabled. Uniforms
and sampler bindings are captured for each queued draw, so a shared material
can be updated between parts. Normal transforms and reflected front-face
selection follow the retained shader contract above.

`GltfLoadOptions::max_texture_size` caps each uploaded base-color, emissive,
normal, and metallic-roughness image while preserving aspect ratio. Occlusion
maps are not uploaded. Full-resolution decoding still
happens first; it is not a bound on peak CPU memory. To reject oversized sources
before pixel decoding and texture allocation, add per-image limits without
changing an existing `GltfLoadOptions` literal:

```rust,no_run
use macroquad::gltf_draw::{GltfImageLimits, GltfLoadOptions, GltfModel};
# fn load(bytes: &[u8]) -> Result<GltfModel, String> {
let settings = GltfLoadOptions {
    max_texture_size: Some(512),
    ..Default::default()
}.with_image_limits(GltfImageLimits {
    max_source_pixels: Some(2048 * 2048),
    max_decoded_bytes: Some(16 * 1024 * 1024),
});
let model = GltfModel::from_bytes_with_settings(bytes, settings)?;
# Ok(model) }
```

The limits check source width × height and its checked RGBA8 byte estimate
(four bytes per pixel). They apply to each sampled base-color, emissive,
normal, and metallic-roughness image, not their sum. The loader preflights
consumed images before decoding or uploading any of them; errors identify the
source image and material. Unused images and occlusion maps are skipped.
Existing constructors and default limits remain uncapped.
For the separate CPU scene path, use
`GltfScene::from_bytes_with_image_limits(bytes, limits)`; this checks its sampled
base-color images and does not resize them.
`GltfMaterialMaps::from_bytes_with_image_limits` checks sampled normal and
metallic-roughness images the same way and does not upload them.

These are rejection limits, not a decoder-memory sandbox. Decoder scratch
buffers, higher-bit-depth intermediates, resizing, encoded inputs, geometry
and the sum of retained images can consume additional memory. Prepare oversized
images offline and load the prepared GLB. Lowering `max_texture_size` alone
still cannot remove full-resolution decode peaks.

`GltfModelStats` contains
load counts, not measured memory or per-frame GPU costs. `textures` counts
base-color and emissive uploads only. Normal and metallic-roughness images,
their UV selection and transforms, and tangents are retained by
`GltfModel::from_bytes_with_material_maps` and `GltfMaterialMaps`.
`submit_shaded` samples those maps. CPU morph evaluation and independent
deforming draw instances and optional TRS/weight clips are described below.
Shader-based morphing, skinning, environment lighting, shadows and transmission
remain roadmap work.

## Opt-in glTF material passes

`GltfMaterialRenderer` provides a base-color/emissive preview and optional basic
lighting using the existing loaded data. It leaves `GltfModel::draw`, retained
draws and custom shaders unchanged. Construct one renderer in a running
application, set the camera, then collect **every instance for that view into
one frame** before drawing:

```rust,no_run
use macroquad::prelude::*;
# fn render(model: &RetainedGltfModel, camera: &Camera3D) -> Result<(), String> {
let mut renderer = GltfMaterialRenderer::new().map_err(|e| e.to_string())?;
set_camera(camera);
let mut frame = renderer.begin(camera.position, camera.target - camera.position)?;
frame.submit_retained(model, Mat4::IDENTITY)?;
frame.submit_retained(model, Mat4::from_translation(Vec3::X))?;
let stats = frame.draw();
assert_eq!(stats.skipped_draws, 0);
# Ok(()) }
```

Keep the renderer across frames in a real loop. `submit` accepts a CPU
`GltfModel`; `submit_retained` shares uploaded geometry. Collection borrows the
models until `draw` or frame drop. Dropping an undrawn frame cancels its
collection. Instance transforms must be finite and affine. The current
low-level model stack at `draw` is included in both positions and sorting;
the camera/target must stay consistent with the view passed to `begin`.

For visibility filtering or independently moving a chunk, use `submit_part`
with a borrowed `GltfDrawMesh`, or `submit_retained_part` with a borrowed
`RetainedGltfDrawMesh`. Keep one loaded model and omit hidden parts:

```rust,no_run
use macroquad::prelude::*;
# fn selected<'a>(frame: &mut GltfMaterialFrame<'a>, model: &'a RetainedGltfModel) -> Result<(), String> {
for part in &model.parts {
    if part.name == "collected_cell" { continue; }
    let offset = if part.name == "gate" { Vec3::Y * 2. } else { Vec3::ZERO };
    frame.submit_retained_part(part, Mat4::from_translation(offset))?;
}
# Ok(()) }
```

These calls borrow the existing chunk without cloning model metadata or
geometry; retained draws reuse uploaded buffers. The additional affine matrix
acts on already baked scene coordinates, so rotation around a chosen pivot
still needs `T(pivot) * R * T(-pivot)`. No rest pose or automatic pivot is
inferred by these additional-transform methods. Selected parts share factors, lighting, validation and stable global
transparency sorting with whole-model submissions in the same frame. Submitting
a model and one of its parts draws that part twice; omission is caller controlled.

Opaque and masked chunks draw first with depth testing and depth writes. Opaque
alpha is ignored. Mask fragments below the exact floating-point cutoff are
discarded; surviving fragments are opaque and write depth. Blended chunks then
draw back to front across all submitted models with depth testing, no depth
writes and straight-alpha blending. Sorting uses transformed chunk centers
along the camera forward direction, with submission order breaking ties.
Intersecting triangles, overlapping chunks and transparent surfaces that cross
each other cannot always be ordered correctly by a single center. Split such
geometry or supply a custom renderer; this is not order-independent transparency.
Separate collections cannot sort transparency across each other.

Single-sided materials cull back faces, including reflected instance transforms;
double-sided materials draw both faces. Colors use the exact `GltfSurface`
factors, ignoring the factor already baked into vertex colors. Imported vertex
`COLOR_0` is not available in the existing loader and is reported by the warning
inspector. Base/emissive texture RGB is decoded with the piecewise sRGB transfer
function before multiplying linear factors; alpha stays linear. Ordinary
materials preview base plus emissive without lighting. `KHR_materials_unlit`
uses base color only, as required by the extension. Missing textures are white.

The sidedness guarantees above apply to OpenGL/WebGL. The pinned miniquad
0.4.11 Metal backend ignores front-face and cull pipeline settings, so
single-sided materials are not culled there. A Metal shader is supplied, but
has not been executed in this round; Metal parity is not established.

### Optional basic lighting

Each collection starts in the existing preview mode. Call
`frame.set_lighting(Some(GltfLighting { .. }))?` to apply hemisphere ambient
and one directional diffuse light to ordinary materials. The parameters are
plain values; no scene engine or light objects are required:

```rust,no_run
use macroquad::prelude::*;
# fn configure(frame: &mut GltfMaterialFrame<'_>) -> Result<(), String> {
frame.set_lighting(Some(GltfLighting {
    hemisphere_up: Vec3::Y,
    sky_color: vec3(0.45, 0.50, 0.60),
    ground_color: vec3(0.15, 0.12, 0.10),
    directional_direction: vec3(-0.35, 0.85, 0.45), // Toward the light.
    directional_color: vec3(0.70, 0.65, 0.55),
}))?;
// To return this collection to the base + emissive preview:
// frame.set_lighting(None)?;
# Ok(()) }
```

Directions are world-space, finite, nonzero vectors; the setter normalizes them.
Invalid parameters return an error and leave the previous lighting unchanged.
Colors are finite, nonnegative **linear RGB**, with brightness included in their
values. Set both hemisphere colors to zero to disable ambient, or set the
directional color to zero to disable the directional light. `GltfLighting::default()`
uses Y-up, sky `(0.6, 0.6, 0.6)`, ground `(0.2, 0.2, 0.2)`, a direction toward
`(1, 2, 1)`, and directional color `(0.8, 0.8, 0.8)`. Supplying these defaults
still requires opting in with `Some` for each collection.

For a unit world normal `N`, the hemisphere weight is `0.5 + 0.5 * dot(N, up)`;
the directional weight is `max(dot(N, toward_light), 0)`. `submit` and
`submit_part` add these light contributions, multiply the linear base color by
the result, then add the textured emissive contribution. Emission stays
independent of the lights, so dark areas of an emissive texture remain dark.
The exact base and emissive factors are still applied once. Those calls do not
use metallic or roughness factors and do not sample normal or
metallic-roughness maps.

`submit_shaded`, `submit_part_shaded`, `submit_retained_shaded` and
`submit_retained_part_shaded` use the same lights with a directional GGX
response. A missing metallic-roughness map is white, so the surface factors
pass through: green is roughness and blue is metallic, both linear, and red
occlusion is ignored. A missing normal map is a flat `(128, 128, 255)` texel,
not white. Sampled normals are linear, `rgb * 2 - 1`, then `normalTexture.scale`
on XY. Tangents use the forward vector transform for both the baked node matrix
and the additional instance matrix. The geometric normal uses the inverse
transpose. A mirrored instance flips the bitangent. The view vector
is `normalize(camera_position - world_position)`, so an orthographic camera
still gets a perspective view. `KHR_materials_unlit` and a frame whose lighting
is `None` ignore the maps and stay on the preview path. There is no image-based
lighting, shadow map or transmission. Retained draws need a `GltfShadingModel`
built with `GltfShadingModel::from_model` while the CPU mesh still exists; that
upload does not change the retained position buffers.

Imported normals use the inverse transpose of the composed parent/instance
matrix, including nonuniform scale. Reflections preserve logical front faces;
double-sided back faces reverse the lighting normal. A singular composed
matrix cannot supply inverse normals, so ordinary lit chunks are skipped and
counted in `skipped_draws`. Preview and `KHR_materials_unlit` chunks do not need
inverse normals. A zero normal receives the midpoint of the hemisphere colors
and no directional contribution. The unlit extension always uses only base
color, even when lighting is selected. Existing current-material draw methods
and individual part draws remain available for custom lighting, transforms
and effects.

The default `GltfColorOutput::Srgb` encodes the final RGB for a normal display
target. Blending then happens in that target's encoded color space, as with
ordinary macroquad drawing. For linear compositing, call
`frame.set_output(GltfColorOutput::Linear)`, draw into a linear render target,
and apply one sRGB conversion in your final full-screen resolve. The renderer
does not create that target, change its format, perform tone mapping or provide
HDR storage. Avoid double conversion with an automatically encoding target.
Existing RGBA8 textures and mipmaps filter their encoded values before the
shader decodes them; neither output mode changes this texture-filtering limit.

`draw` flushes the collected GPU work before returning, restores the model stack
and selects the default material. This makes dropping the renderer afterward
safe; select your own material again before further custom drawing. It does not
advance a frame, change the camera or clear the target. Keep custom materials
used by earlier queued draws alive until this flush completes, even after
switching to a different material. Environment reflections, occlusion,
refraction and shadows are separate work. Normal and metallic-roughness maps
are sampled only by the `submit_*_shaded` methods above.

Inspect source bytes once to get structured warnings without opening a window
or decoding images/geometry:

```rust,no_run
use macroquad::prelude::*;
# fn inspect(bytes: &[u8]) -> Result<(), String> {
for warning in gltf_material_warnings(bytes)? {
    println!("{}: {:?} ({:?})", warning.path, warning.material_name, warning.feature);
}
# Ok(()) }
```

Warnings identify the loaded/source node, node path, primitive and material;
unsupported map warnings also identify their source image when available.
They describe the source bytes, so call-site material edits are not reflected.
Unused materials and nodes outside the selected scene are omitted. Non-unlit
materials retain the `Lighting` warning: the shaded path is still not glTF's
full lighting model. This source inspection does not know the lighting chosen
later for a collection. Normal and metallic-roughness maps are no longer
warnings. Occlusion, transmission and other material extensions remain
warnings; loader errors (such as unsupported UV mappings) still fail loading
normally.
See `examples/gltf_materials.rs`, which submits whole models, and the tiny
native/browser pixel fixture `examples/gltf_material_check.rs`, which also
checks selected-part filtering, independent CPU and retained transforms, and
mixed transparency order.

## Replacing an imported node pose

Use `GltfModel::from_bytes_with_pose` to retain authored node world matrices
alongside the world-baked chunks. For load limits, incremental loading or
material maps, call `GltfModelLoader::take_pose` after completion alongside
`take_model` and `take_material_maps`. All three results reuse the same parse;
the pose sidecar includes empty nodes and a spawn's full authored rotation.

```rust,no_run
use macroquad::prelude::*;
# fn moved(bytes: &[u8], renderer: &mut GltfMaterialRenderer) -> Result<(), String> {
let (cpu, rest) = GltfModel::from_bytes_with_pose(bytes)?;
let mut pose = rest.node(cpu.parts[0].node)?;
let model = RetainedGltfModel::from_model(cpu, false).map_err(|e| e.to_string())?;
// This is the complete replacement world matrix, with an authored node pivot.
pose.set_world(Mat4::from_translation(vec3(2., 0., 0.))
    * Mat4::from_rotation_y(0.7))?;
let part = &model.parts[0];
let bounds = part.posed_bounds(&pose, Mat4::IDENTITY)?;
let mut frame = renderer.begin(vec3(0., 2., 6.), -Vec3::Z)?;
frame.set_lighting(Some(GltfLighting::default()))?;
frame.submit_retained_part_posed(part, &pose)?;
frame.draw();
# let _ = bounds;
# Ok(()) }
```

`GltfNodePose` caches `world * inverse(rest_world)` when `set_world` is called.
It removes the baked node placement before applying the replacement, so a
non-origin object rotates around its authored node origin. Set the replacement
to a spawn node's `rest.rest_world(spawn.node)` to keep its yaw as well as its
position. A replacement is the complete node world matrix, including any
desired scale; it replaces the old scale too. Setting it back to
`pose.rest_world()` restores exact identity drawing. The sidecar's node IDs
belong to its source model; matching numbers from another file are unrelated.

CPU `submit_part_posed` and retained `submit_retained_part_posed` use the existing
material collection, including inverse-transpose normals, reflected winding,
parent transforms and global transparency sorting. The pose is copied at
submission, so later edits affect subsequent submissions. Retained buffers are
reused and no per-frame vertex vectors are cloned. For normal/roughness-map
shading, pass `pose.transform()` to the existing selected shaded submission
after selecting the part with the matching node. `draw_posed` on either chunk
uses the current material and follows the custom normal/culling contract above.

`posed_bounds(&pose, parent)` conservatively transforms the stored eight bound
corners for culling. Supply the full parent matrix; the pure query does not
read the model stack. The renderer sorts the correspondingly transformed chunk
center. Singular rest matrices cannot recover lost local geometry, so creating
that node's editable pose returns an identified error while ordinary loading
and drawing remain available. Singular replacement matrices work for bounds
and unlit drawing; ordinary lit draws skip them. Invalid updates leave the
previous pose intact.

When reorienting the source model, apply the same `with_up_axis(up)` to the
original pose sidecar before creating editable poses. A standalone
`GltfNodePose::set_world` changes just that selected node. Use a hierarchy
instance to propagate local edits to descendants.

## Editing an imported hierarchy

`GltfModelPose::hierarchy()` creates an independent `GltfHierarchyPose` using
the authored local matrices and parent relationships. The source/rest metadata
is shared, while each instance owns its current local and world matrices.
Cloning an instance copies its editable state and continues to share that
metadata; it does not clone mesh vertices or retained GPU buffers.
Loader sidecars preserve exact authored local matrices. A sidecar built later
with `from_scene` may need a parent's inverse to reconstruct a sheared child;
that reconstruction can introduce roundoff or fail under a singular parent.

```rust,no_run
use macroquad::prelude::*;
# fn edit(bytes: &[u8], renderer: &mut GltfMaterialRenderer, camera: &Camera3D) -> Result<(), String> {
let (cpu, rest) = GltfModel::from_bytes_with_pose(bytes)?;
let model = RetainedGltfModel::from_model(cpu, false).map_err(|e| e.to_string())?;
let mut instance = rest.hierarchy()?;
// Obtain a handle once from the source node index (for example, after a name lookup).
let handle = rest.handle(model.parts[0].node).ok_or("missing node")?;
let mut local = instance.local_trs(handle)?;
local.rotation = Quat::from_rotation_y(0.7);
instance.set_local(handle, local)?;
// The same edit updates every descendant; draw all chunks using their node pose.
let frustum = camera.frustum().ok_or("invalid camera")?;
let mut frame = renderer.begin(camera.position, camera.target - camera.position)?;
for part in &model.parts {
    let node = rest.handle(part.node).ok_or("missing chunk node")?;
    let pose = instance.node_pose(node)?;
    let bounds = part.posed_bounds(&pose, Mat4::IDENTITY)?;
    if frustum.intersects_aabb(&bounds) {
        frame.submit_retained_part_posed(part, &pose)?;
    }
}
frame.draw();
instance.reset_local(handle)?; // restore only this node's authored local matrix
instance.reset();              // restore the whole instance exactly
# Ok(()) }
```

`GltfNodeHandle` is stable across edits and clones of the same source metadata;
its `index()` matches the source node ID on draw chunks. Keep it with that source
model: a handle's numeric index does not identify a node in a different file.
Parents and empty nodes can be edited through the same API. A parent local edit
recomputes descendant world matrices, and `node_pose(handle)` supplies the selected draw pose
for any chunk of that node. Each edit validates the full affected hierarchy before
committing it, so a failed edit leaves the previous instance unchanged.

`GltfLocalTransform` stores translation, a unit quaternion and scale. Read it with
`local_trs(handle)` and replace it with `set_local`; nonuniform parent scales and
child rotations compose as matrices, preserving resulting world shear. Authored
local matrices that contain shear must use `local` and `set_local_matrix` instead
of TRS decomposition. `reset_local` restores the exact authored matrix and updates
its descendants. `reset` restores all authored local/world matrices and exact
identity draw transforms.

Bounds remain explicit, conservative queries on each chunk's stored box: obtain
the current `node_pose`, then call `posed_bounds` for culling. Fetch it again after
an edit; an already returned pose is a snapshot. Material-frame posed submissions
use that same additional matrix for normals, reflected winding and global
transparent sorting, and capture it when submitted. For custom drawing, use
`draw_posed` or the snapshot's `transform()` with the existing shaded submission
methods. The scene's stored spawn markers stay at rest. Morph weights and
clip clocks belong to separate instances; applying an animation sample can edit
this hierarchy, but an ordinary hierarchy edit does not advance a player or
change morph weights. Skinning remains unsupported.
`collider_shape(&scene, collider)` returns one imported collider in the current
worlds. The list on `GltfScene` stays at rest, and `trigger` is still that
stored flag, so a slide keeps taking only the solid shapes. The query applies
`posed_world * inverse(rest_world)` to the imported shape. A quarter turn stays
an `Aabb`; any other rotation is an `Obb`. A sphere's radius grows by the
delta's largest stretch, including shear; it encloses the transformed sphere
instead of choosing only the longest transformed coordinate axis. A capsule's
endpoints follow the delta, and its radius
uses the largest stretch perpendicular to the original segment. A default
Blender cube keeps the size already stored on the collider.
`Obb::from_world_matrix` on the node reads the node scale as the full size,
which is half a cube whose vertices sit at ±1. A missing node, or a rest world
that cannot be inverted, returns `None`.

```rust,no_run
use macroquad::prelude::*;
# fn gate_hull(scene: &GltfScene, pose: &GltfHierarchyPose) -> Option<GltfColliderShape> {
let gate = &scene.colliders[0];
pose.collider_shape(scene, gate)
# }
```

Singular rest geometry retains the selected-pose limitations
described above. The self-contained `gltf_pose_check` fixture exercises parented
objects, independent instances, reset, nonuniform scale, bounds/culling and global
transparent ordering on native OpenGL and browser WebGL.

## CPU morph data and evaluation

With `gltf`, `GltfMorphData::from_bytes` loads mesh-local base positions,
normals, POSITION/NORMAL target deltas and default weights without a window,
image decoding or GPU allocation. This additive sidecar leaves `GltfScene`
and draw-model struct literals unchanged. Its selected-scene node and mesh
indices match a `GltfScene` loaded from the same bytes.

```rust,no_run
use macroquad::gltf::GltfMorphData;
use macroquad::math::Mat4;
# fn evaluate_face(bytes: &[u8], face_node: usize) -> Result<(), macroquad::gltf::GltfError> {
let morphs = GltfMorphData::from_bytes(bytes)?;
let face = morphs.evaluate(face_node, 0, Some(&[0.5]))?;
let bounds = face.transformed_bounds(Mat4::IDENTITY)?;
println!("{} deformed vertices, {bounds:?}", face.positions.len());
# Ok(()) }
```

Pass `None` for authored defaults: a node's weights override its mesh's
weights, and missing defaults mean zero. Supply exactly one finite weight per
target. Weights may be negative or greater than one; they are not clamped or
normalized. Evaluation produces independent arrays and bounds, leaving shared
rest data and other instances unchanged. Missing normal deltas contribute zero;
missing base normals remain absent. A contributing normal delta causes the
resulting nonzero normals to be normalized; zero vectors stay zero. Without
one, authored normals are preserved.

Positions and deltas stay in mesh-local coordinates. Apply the matching scene
node's world matrix after evaluation, including any `with_up_axis` rotation.
`transformed_bounds` encloses the evaluated vertices under a
finite affine matrix, including reflection, nonuniform scale and shear. It does
not update scene colliders or draw-model bounds. Use the drawing API below to
keep deforming draw geometry and its bounds together. Optional weight clips
are covered below; tangent deltas and skinning remain separate work.

## Drawing independent morph instances

`GltfMorphModel` loads the same embedded GLB materials and triangle-safe chunks
as the ordinary draw loader, plus shared immutable mesh-local morph input and
exact chunk-to-source vertex mappings. Construct it inside a running macroquad
application. Each `instantiate()` starts with the node's authored default
weights. Instances initially share the immutable default buffers and textures;
changing one instance's weights gives its changed chunks new buffers.

```rust,no_run
use macroquad::prelude::*;
# fn draw_faces(bytes: &[u8], face_node: usize, renderer: &mut GltfMaterialRenderer) -> Result<(), String> {
let source = GltfMorphModel::from_bytes(bytes)?;
let neutral = source.instantiate();
let mut smiling = source.instantiate();
smiling.set_weights(face_node, &[1.0])?;
assert!(!smiling.set_weights(face_node, &[1.0])?); // no evaluation or upload
let mut frame = renderer.begin(Vec3::Z * 5.0, -Vec3::Z)?;
frame.set_lighting(Some(GltfLighting::default()))?;
frame.submit_retained(neutral.model(), Mat4::from_translation(-Vec3::X))?;
frame.submit_retained(smiling.model(), Mat4::from_translation(Vec3::X))?;
frame.draw();
# Ok(()) }
```

The node index is scene-local and must come from this source document. Supply
exactly one finite weight per target, including target slots that contain only
unsupported tangent deltas. Negative weights and extrapolation are allowed.
`set_weights` returns `false` when the requested weights are unchanged. Invalid
weights or failed evaluation leave the instance's weights and geometry intact.
`source.data()` exposes the CPU evaluator; `source.pose()` supplies the matching
rest pose for replacement-node or hierarchy transforms.
`reset_weights(node)` restores that node's authored defaults.
`source.with_up_axis(up)?` creates a converted source before instantiation;
the original source and its existing instances keep their current axes.

`instance.model()` is the current `RetainedGltfModel`. Its chunks contain the
evaluated positions, normals, bounds and centers. Use their `world_bounds` or
`posed_bounds` for culling and submit them to one material frame for global
transparency sorting. Authored normals follow POSITION/NORMAL evaluation and
the node's inverse transpose; missing base normals are generated from the
deformed triangles. UV values and material factors stay unchanged; textures and
rest data remain shared. This path supports existing diffuse material passes
and custom shaders. It does not construct deformed tangent buffers for
normal-map/GGX submissions.

Update weights before borrowing the model for material collection. For an
explicit snapshot, clone `instance.model()`: the clone shares its current
immutable buffers. Later weight updates replace buffers instead of modifying
that snapshot, so an already queued draw keeps its old geometry even when the
instance is updated or dropped. Retained-resource collection frees old buffers
after their queued references are released. Repeating the same weights or
drawing an unchanged instance performs no geometry uploads. A change that
affects only one node leaves other nodes' buffers shared.

For staged construction use `GltfModelLoader::new_with_morph(bytes, settings)`,
poll it as usual, then call `take_morph_model()` once at completion. Synchronous
loading is `GltfMorphModel::from_bytes_with_settings`. These opt-in constructors
retain the CPU rest arrays needed for later updates; ordinary `GltfModel`
loading and existing public struct literals are unchanged. Updates evaluate and
upload synchronously; taking the initial morph model also evaluates defaults and
uploads all initial chunks synchronously. Neither operation provides a
frame-time budget. Collider shapes and spawn markers are unaffected.
Shader-based deformation, tangent morphs and skinning are separate work.

## Playing glTF clips

With `gltf`, `GltfAnimationData::from_bytes` loads an optional CPU sidecar from
the same GLB or embedded-buffer glTF as the model. It does not decode images,
upload geometry, or start playback. Clips retain immutable shared key arrays
and map document node IDs to the selected scene's preorder, matching draw and
hierarchy IDs. Keep clips, poses and morph instances from the same source.

Translation, rotation, scale and morph-weight channels support LINEAR and STEP.
LINEAR rotations use normalized shortest-path spherical interpolation. STEP
switches at the exact key time. Each channel clamps to its own first/last key,
including before a delayed first key. Clip time starts at zero and duration is
the last key across all channels. CUBICSPLINE returns an explicit import error;
skinning, animation extensions, clip blending and events remain separate work.
Outputs support floats and glTF's normalized integer rotation/weight encodings,
including sparse accessors. Invalid formats, ranges, key times, duplicate
targets and malformed sparse indices return errors.

```rust,no_run
use macroquad::prelude::*;
# fn animate(bytes: &[u8]) -> Result<(), String> {
let source = GltfMorphModel::from_bytes(bytes)?;
let mut face = source.instantiate();
let mut hierarchy = source.pose().hierarchy()?;
let clips = GltfAnimationData::from_bytes(bytes).map_err(|e| e.to_string())?;
let clip = clips.clips.first().ok_or("no animation clip")?.clone();
let mut player = GltfAnimationPlayer::new(clip);
player.set_looping(true);
let sample = player.advance(1. / 60.).map_err(|e| e.to_string())?;
sample.apply(&mut hierarchy, Some(&mut face))?;
for part in &face.model().parts {
    let handle = hierarchy.handle(part.node).ok_or("missing node")?;
    part.draw_posed(&hierarchy.node_pose(handle)?)?;
}
player.set_paused(true); // advance now holds time; seek still works.
player.reset(); // Rewind the player, preserving pause/loop settings.
player.sample().map_err(|e| e.to_string())?.apply(&mut hierarchy, Some(&mut face))?;
# Ok(()) }
```

`clip.sample(seconds)` samples absolute finite clip time without a player.
`player.seek(seconds)` uses absolute nonnegative time; `advance(delta_seconds)`
uses a nonnegative elapsed interval supplied by the application. A nonlooping
player clamps at duration; looping wraps into `[0, duration)`. Zero-duration
clips stay at zero. Pausing holds advancement, and reset rewinds to the clip's
first sample rather than the model's authored rest pose. Apply the returned
sample explicitly. To restore authored rest data, use hierarchy/morph reset.

`sample.apply_pose`, `apply_morph` and combined `apply` stage updates and commit
only after validation succeeds. TRS channels replace the animated node's full
local transform, using authored values for its unanimated components. Nodes
absent from the clip retain their current values. Zero scale can be sampled and
later restored without decomposing the singular current matrix. TRS channels
cannot target matrix-authored nodes. Weight-only channels can.

Apply `clip.with_up_axis(up)` with the same conversion as the source model and
pose before creating the player. Animated roots receive that conversion;
children remain parent-local. Pose-aware bounds, culling and material submission
still use `hierarchy.node_pose` on the current morph model. Ordinary model
drawing does not read the hierarchy automatically. Morph samples reuse the
existing synchronous CPU evaluation and immutable replacement buffers, with
no upload for unchanged weights; choose the update frequency in the application.
The self-contained `gltf_animation_check` fixture drives a child transform and
morph weight together on native and WebGL. The `GltfAnimationPlayer` rustdoc
also contains a complete CPU-only one-second translation clip, including its
embedded buffer, pause, seek and loop assertions.

### Update order for an animated level

Keep the source model, CPU scene, rest pose and clips from the same document,
with the same up-axis conversion. These APIs deliberately have separate owners:

| State | API that changes it | Reset behavior |
| --- | --- | --- |
| Playback clock | `GltfAnimationPlayer::advance` or `seek` | `reset` rewinds to time zero, preserving pause and loop settings |
| Current node locals/worlds | `GltfHierarchyPose::set_local` or a sample's `apply_pose`/`apply` | `reset` restores exact authored matrices |
| Current deformed geometry | `GltfMorphInstance::set_weights` or a sample's `apply_morph`/`apply` | `reset_weights(node)` restores that node's authored weights |
| Stored scene markers | `GltfScene` import | Pose and morph updates leave this list at rest |

In each application frame:

1. Advance the player with the application's elapsed seconds and apply the
   sample to the matching hierarchy and, when needed, morph instance. A
   transform-only sample can use `apply_pose` without loading a morph model.
2. Query `hierarchy.collider_shape(scene, collider)` for solid and trigger
   markers that should follow the hierarchy. Keep the stored `trigger` flag
   when selecting solid responses. Morph deformation does not reshape these
   imported marker primitives.
3. Borrow the current morph model (or the ordinary retained model for a
   transform-only animation), obtain each chunk's current `node_pose`, and use
   `posed_bounds` for culling. Submit those same posed chunks to one material
   frame so transparency can sort across instances.

`sample.apply` returns whether **morph weights** changed; `false` can still mean
that transforms changed successfully. It commits pose and morph state together
after validation, but it does not rewind a player already advanced by the
caller if applying fails. A cloned player can preserve a clock checkpoint when
the application needs to retry. Already captured node poses and retained-model
clones remain snapshots; fetch them again after an update for current drawing
and collision queries. Unchanged morph weights avoid geometry uploads, while
changed weights still evaluate and upload synchronously.

## Draw a HUD over a level

Finish the world's material pass before switching to the default material and
camera for a window HUD. Clear the background before drawing the world;
`clear_background` clears the surface even inside a clip scope. A clip bounds
immediate drawing, while built-in widgets queue their own drawing for frame
completion. Use a UI group for scrolling widgets and `with_clip_rect` for
custom text or images.

```rust,no_run
use macroquad::prelude::*;
# fn draw_frame(camera: &Camera3D) {
clear_background(BLACK);
set_camera(camera);
draw_cube(Vec3::ZERO, Vec3::ONE, None, BLUE);
// Finish any GltfMaterialFrame here, before selecting the HUD camera.
gl_use_default_material();
set_default_camera();
let log = Rect::new(12.0, 48.0, (screen_width() - 24.0).max(0.0), 72.0);
with_clip_rect(log, || {
    draw_rectangle(log.x, log.y, log.w, log.h, Color::new(0.0, 0.0, 0.0, 0.6));
    draw_text("Goal: reach the beacon", log.x + 8.0, log.y + 24.0, 20.0, WHITE);
});
draw_text("Mission", 12.0, 30.0, 24.0, WHITE); // previous clip is restored
# }
```

Window HUD rectangles and `mouse_position_logical()` use logical pixels.
`with_clip_rect` uses texture pixels when a render target is active; select
the camera and target before entering it and keep them fixed until it returns.
`Camera2D::world_to_target` instead always returns physical surface pixels.
For a minimap drawn into a texture, follow the
[displayed-target picking recipe](ui-2d-guide.md#pick-a-displayed-render-target)
to map a logical window click into target pixels and then world coordinates.
The legacy screen methods retain their window-sized behavior.

Build menus with [layout helpers](ui-2d-guide.md#size-rows-columns-and-scrolling-settings)
and [stable button IDs](ui-2d-guide.md#make-buttons-usable-from-the-keyboard).
Borrow `root_ui()` once for that UI block and release the borrow before
`next_frame().await`. A modal contains built-in UI focus and pointer input; the
application still gates world input, movement and animation while paused.
For example, skip `GltfAnimationPlayer::advance` during a pause. Custom HUD
drawing inside `with_clip_rect` does not automatically clip deferred widgets.

## Logical and physical input

Use `mouse_position_logical()` and `touches_logical()` for HUD hitboxes drawn
with `screen_width()` / `screen_height()`. Both use logical window coordinates,
with the origin at the top left and positive Y downward. Their physical
counterparts, `mouse_position_physical()` and `touches_physical()`, use
framebuffer pixels. `logical_to_physical` and `physical_to_logical` convert
positions or displacements using the current `screen_dpi_scale()`.

For mouse look, `mouse_delta_logical()` gives frame displacement in logical
pixels, positive right/down; `mouse_delta_physical()` gives framebuffer pixels.
A resize or density change resets relative history so a stationary pointer
cannot rotate the camera. Browser relative movement is scaled to framebuffer
pixels before Rust receives it; fractional pixels accumulate across events.

```rust,no_run
use macroquad::prelude::*;
# fn input(yaw: &mut f32, button: Rect) {
*yaw -= mouse_delta_logical().x * 0.002;
let mouse_over_button = button.contains(mouse_position_logical());
let touch_over_button = touches_logical().iter()
    .any(|touch| button.contains(touch.position));
let framebuffer_point = logical_to_physical(mouse_position_logical());
# let _ = (mouse_over_button, touch_over_button, framebuffer_point);
# }
```

Existing functions keep their historical units: `mouse_position()` returns a
logical tuple; `mouse_delta_position()` returns inverted normalized movement;
`touches()` returns physical positions and `touches_local()` retains its legacy
normalization. New code should choose an explicit coordinate API. Coordinate
conversion does not configure canvas sizing, density caps or frame pacing.
`Camera3D::screen_to_ray` expects the logical window point, or the render
target's pixel size when one is set.

## Pointer capture

`set_cursor_grab(true)` asks the platform to capture the cursor.
`cursor_capture()` reports the current `CursorCapture`: `Idle`, `Requested`,
`Active`, `Denied`, or `Lost`. `cursor_capture_events()` lists the states
entered since the previous frame, oldest first, and is cleared at the end of
the frame. Reading it does not drain the list. The frame loop does not call
`set_cursor_grab` again.

On desktop the grab is synchronous. `set_cursor_grab(true)` returns with
`cursor_capture() == CursorCapture::Active`, and that frame's events are
`Requested` then `Active`. Desktop code does not observe `Denied`. Focus loss
releases the grab once and becomes `Lost`. Focus returning does not grab
again; call `set_cursor_grab(true)` for a new request.

In the browser, `js/mq_js_bundle.js` tracks the Pointer Lock promise. A request
without a user gesture becomes `Denied`. Escape, a rejected promise, and focus
loss do not call `requestPointerLock` again. While the last
`set_cursor_grab(true)` still stands, the next primary pointer or touch press
asks once, inside that event. `set_cursor_grab(false)` cancels the retry.
Calling `set_cursor_grab(true)` on later frames does not ask again until one
of those presses. An older bundle without the capture plugin reports only
`Idle` or `Active`.

Use `mouse_delta_logical()` for mouse look while capture is `Active`. Positive
X is right and positive Y is down. `examples/cursor_capture.rs` prints the
state. `G` grabs, `Tab` releases, and `Q` quits.

```rust,no_run
use macroquad::prelude::*;
# async fn look(yaw: &mut f32) {
if cursor_capture() != CursorCapture::Active {
    if is_mouse_button_pressed(MouseButton::Left) {
        set_cursor_grab(true);
    }
} else {
    *yaw -= mouse_delta_logical().x * 0.002;
}
# next_frame().await;
# }
```

## Queued font atlases

Growing a font atlas, or dropping the last `Font` handle, keeps the texture
already captured by text queued for this frame. Those textures are collected
after the frame is submitted, on the same boundary as other unused textures.
No extra font clone or manual flush is required for that case. Atlas packing,
glyph rasterization, and the public `Font` methods are unchanged. `draw_text_ex`
stores each glyph's texture id and pixel rect, so an atlas that grows
mid-string keeps the earlier glyphs on the previous texture.

UI widgets share one atlas and one texture bind for the frame. Their commands
store sprite keys, and UVs (including nine-slice margins) are taken from the
CPU image when the frame is rasterized, after every widget has cached glyphs.
A larger font later in the same frame can grow that image; earlier labels
still sample the repacked rects. Text positions stay in logical pixels,
including when the display density is not 1.
`examples/font_atlas_lifetime.rs` forces several atlas replacements and drops
the font before flush. `cargo test --test font_atlas_lifetime` is the same
GPU check.

## Browser canvas budgets

Native windows keep using `Conf`. The browser host can cap density, framebuffer
pixels, the longest framebuffer edge, and the requested frame rate. Set
`window.macroquadConfig` before `load("app.wasm")`, or replace it later with
`window.macroquadHost.configure`. One effective scale keeps the framebuffer,
projection, text, and logical/physical input together. `targetFps` is a maximum
requested rate. Display refresh, throttling, and rendering cost can make the
achieved rate lower, and a late frame does not produce catch-up frames.
[Browser canvas configuration](browser-host.md) lists the fields, the
requested-versus-effective report, and the rounding limits.
`examples/canvas_host.rs` is the self-contained fixture.

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

`Camera3D::screen_to_ray` unprojects a screen point into a `Ray`. The point is in pixels of the surface the camera draws to, origin at the top left, y down. A window uses logical pixels: `mouse_position_logical()` (the same units as `mouse_position()` and `screen_width()`). `mouse_position_physical()` is framebuffer pixels and misses the center when the display density is not 1. A render target uses the texture's pixel size instead of the logical window. A viewport is that camera's GL viewport, converted the same way as `Camera2D::world_to_screen`, in the same pixel space as the surface. The direction is a unit vector. A perspective ray starts at `position`, so `t` is the distance from the camera. An orthographic ray starts on the near plane and stays parallel to the look direction. `examples/picking_ray.rs` highlights the box under the mouse. `examples/ray_cast.rs` still builds its ray by hand.

The viewport changes the pixel rectangle, not the projection aspect. Set
`aspect` explicitly when a subviewport should use its own width/height ratio;
otherwise the camera uses the whole framebuffer ratio. For an orthographic
camera, `fovy` is the full vertical span. An invalid point or camera returns a
ray at `position` with a zero direction. The example below needs no window
because it supplies both `aspect` and a viewport.

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

## Uniform grid candidates

`UniformGrid` keeps world-space AABB snapshots under caller-chosen `u32`
indices. It is available without optional features through `math` and the
prelude. The caller retains the shapes, trigger flags and gameplay state.
Choose a finite positive cell size in the same units as the bounds. The grid
indexes the inclusive cell range from `floor(min / cell_size)` through
`floor(max / cell_size)`, including negative coordinates. Queries on cell
faces, edges and corners include touches.

```rust
use macroquad::prelude::*;

let mut grid = UniformGrid::new(2.0).unwrap();
let wall = Aabb::from_min_max(vec3(1.0, -2.0, -2.0), vec3(5.0, 2.0, 2.0));
assert!(grid.insert(42, wall)); // spans several cells, returned only once
let probe = Aabb::from_min_max(Vec3::ZERO, vec3(1.0, 1.0, 1.0));
assert_eq!(grid.query_aabb(&probe), vec![42]); // closed-boundary touch
let ray = Ray::new(Vec3::ZERO, Vec3::X * 2.0);
assert_eq!(grid.query_ray(&ray, 0.5), vec![42]); // endpoint at x = 1

// Reinserting an index replaces its old bounds. Rebuild after a full update.
let moved = Aabb::from_center_size(vec3(8.0, 0.0, 0.0), Vec3::ONE);
assert_eq!(grid.rebuild([(42, moved), (7, probe)]), 2);
assert_eq!(grid.query_aabb(&probe), vec![7]);
grid.clear();
assert!(grid.is_empty()); // cell size remains 2.0
```

Both queries return unique indices in ascending numeric order. They filter
cell candidates against the stored AABBs, so sharing a cell alone is not a
hit. Floating-point ray filtering can retain extra candidates at extreme
coordinate scales. Bounds are still only a broadphase for an OBB, sphere, capsule or mesh:
run the appropriate shape query on each candidate. Ray results are ordered
by index, not distance; use the actual shape hits to choose the nearest one.
The ray interval is closed, `0 <= t <= max_t`, for
`origin + direction * t`. Direction need not be normalized: `max_t` is a
world distance only for a unit direction. Positive infinity requests the
whole forward ray. A zero direction or zero limit tests the origin.

Construction rejects zero, negative or non-finite cell sizes. Bounds must
be finite and ordered on every axis; point and flat boxes are valid.
An invalid insertion returns `false` without replacing an existing entry.
An invalid query returns an empty vector; ray origin/direction must be finite
and the limit must be nonnegative and not NaN. `rebuild` first clears the grid,
skips invalid entries and retains the last valid bounds for each repeated
index; its return value is the final number of distinct entries.

Large bounds and extreme coordinates remain queryable through a fallback
scan instead of allocating an unbounded number of cells. Large or infinite
ray queries can also scan all entries. The finite ray path selects cells from
its segment's enclosing AABB, then filters with a slab intersection. This is
a small spatial index, without a BVH or mandatory physics world; no particular
frame-time improvement is promised. Reinsert changed bounds, or rebuild, after
moving shapes. For one straight translation, query the union of its starting
and ending AABBs, then use the existing full-body sweeps on the candidates.

### Candidate bounds for sliding

A slide can change direction after hitting a surface. Its path can leave the
union of the requested start and end bounds, so that straight-sweep envelope
is insufficient for `slide_sphere_solids` or `slide_spheres_solids`. At ordinary
game scales, a conservative movement envelope expands the initial body's full
AABB by the requested path length `delta.length()` in every direction, with
an outward numerical margin. For a hull, include every sphere and its radius.
Filter the returned indices into the shape slices that the slide function takes.

Initial-overlap recovery is separate: a push-out can travel farther than the
requested delta, including when that delta is zero. Recover against the complete
solid set first, update the position and bounds, then query candidates around
the recovered body. If overlap remains unresolved, keep the full-solid path
instead of assuming the movement envelope bounds recovery. A simple game can
use the full solid list throughout. Validate any application broadphase against
that complete list for the level's movements, deflections and spawn overlaps.

Stepping and ground snapping likewise need bounds covering their additional
paths; the straight-sweep envelope alone is not a complete character-controller
candidate policy. See [rigid sphere groups](#rigid-groups-of-spheres) and
[character movement](#character-movement) for the underlying movement contracts.

## Frustum culling

`Camera3D::frustum` returns the six inward planes of the same view-projection
used for drawing, or `None` for an unusable camera. An explicit `aspect` permits
CPU-only queries. `Frustum::from_view_projection` accepts a GL clip-space matrix
(`clip = matrix * world`, NDC z in `-1..=1`). The plane order is left, right,
bottom, top, near, far. Points on the boundary are inside.

`contains_point`, `contains_sphere`, and `contains_aabb` require the entire
shape to be inside. Use `intersects_sphere` or `intersects_aabb` for rendering:
these conservative plane tests can keep a shape outside a frustum corner, but
do not reject one meeting the volume. They do not draw or change the camera.

```rust
use macroquad::prelude::*;

let camera = Camera3D {
    position: vec3(0.0, 0.0, 5.0),
    target: Vec3::ZERO,
    up: Vec3::Y,
    aspect: Some(1.0),
    z_near: 0.1,
    z_far: 20.0,
    ..Default::default()
};
let frustum = camera.frustum().expect("valid camera");
let bounds = Aabb::from_center_size(Vec3::ZERO, Vec3::ONE);
assert!(frustum.intersects_aabb(&bounds));
```

`RetainedGltfModel::draw_culled` applies this test to each transformed chunk.
It neither sorts transparency nor performs occlusion culling. The
`frustum_cull` example shows the tested volume separately from the viewing
camera.

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

### Triangle mesh surfaces

`TriangleMesh::new(Vec<Triangle>)` owns finite world-space corners and caches
tight per-triangle and overall AABBs. It returns `None` for non-finite corners;
an empty mesh is valid, with `aabb() == None`. `triangles()` and
`triangle_bounds()` are immutable slices in original index order. Build a new
mesh when geometry moves or changes; no renderer, glTF import, rigid-body
shape variant or automatic collision response is involved.

```rust
use macroquad::prelude::*;

let ramp = Triangle::new(Vec3::ZERO, vec3(2.0, 0.0, 2.0), vec3(0.0, 2.0, 2.0));
let mesh = TriangleMesh::new(vec![ramp]).unwrap();
let hit = mesh.ray_intersect(&Ray::new(vec3(0.5, 0.5, 5.0), -Vec3::Z * 2.0)).unwrap();
assert_eq!(hit.index, 0);
assert_eq!(hit.hit.t, 2.0);
assert!(mesh.overlaps_aabb(&Aabb::from_center_size(vec3(0.5, 0.5, 1.0), Vec3::splat(0.1))));
assert!(mesh.overlaps_sphere(&Sphere::new(vec3(0.5, 0.5, 1.0), 0.1)));
```

The ray result reuses `ClosestHit`: `index` identifies the original triangle,
exact ties keep the first index, and `hit.t` is distance only for a unit ray
direction. It delegates to the existing two-sided triangle ray query, including
winding normals, zero-area misses and its floating-point limits. Ray queries
scan the triangles directly to preserve that query's boundary behavior.

Overlap queries first reject cached bounds, then test the surface using f64
intermediates (triangle/box separating axes or sphere/surface distance). Touches
count, including degenerate triangle segments and points. Non-finite or inverted
boxes and non-finite/negative-radius spheres miss. These are surface queries:
being wholly inside a closed mesh without touching a face does not overlap.
Sphere/edge tests measure perpendicular distance without reconstructing a
closest point, so small positions on long edges spanning the origin survive.
The f64 calculations are still rounded, not exact geometric predicates;
extreme aspect ratios and nearly collinear faces can lose precision.
All queries are linear scans, with no BVH, solid-volume test, sweep or mesh
response solver. The pure non-axis-aligned fixture and query regressions live
in `tests/triangle_mesh.rs` and run without a window.

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

`Ray::closest_capsule` matches the other slice queries. `examples/capsule_overlap.rs` moves one with the arrow keys. `draw_capsule_wires` outlines it: sphere wires at `a` and `b`, and four lines `radius` out from the inner segment.

## Contact

`Contact` is one overlapping pair: `normal`, `penetration`, and `point`. It does not move either shape. The normal points from the second shape toward the first. A pair that only touches still returns a contact with penetration `0`, so a sphere sitting on a floor is a contact. Separated shapes return `None`.

`Aabb::contact`, `Sphere::contact`, `Sphere::contact_aabb`, `Sphere::contact_obb`, `Capsule::contact_sphere`, and `Capsule::contact_aabb` are the queries. `Sphere::contact_obb` rotates into the box, uses the sphere/AABB query, and rotates the point and normal back to world space. Mass and impulses are not here. A capsule whose inner segment runs through a thin box is separated on the short face, not along the segment.

```rust
use macroquad::math::{Aabb, Sphere, Vec3, vec3};

let ball = Sphere::new(vec3(0.0, 0.5, 0.0), 0.5);
let floor = Aabb::from_min_max(vec3(-1.0, -1.0, -1.0), vec3(1.0, 0.0, 1.0));
let contact = ball.contact_aabb(&floor).unwrap();
assert_eq!(contact.penetration, 0.0);
assert_eq!(contact.normal, Vec3::Y);
```

## Sweep

`Sphere::sweep_sphere` moves a complete sphere against a static sphere. Its
world-space normal points out of the static sphere; its point lies on that
sphere, including initial contacts, matching `Sphere::contact`. New impacts
also lie on the moving sphere. Initial penetration deeper than `1e-4` reports
zero; touching/shallow pairs and future hits require inward displacement
greater than `1e-5`. Exact future tangencies miss. Coincident centers use +X.
Zero radii are allowed, but a crossing between two separated points has no
inward surface and misses. Invalid inputs, overflowing radius/separation
intermediates and nonfinite results miss. The existing slide cast supplies
new impact times without changing slide behavior; very small motion or
extreme-scale cancellation can miss. See its rustdoc for numerical limits.
Run the asset-free, window-free `cargo run --example sphere_sweep` for a
graze that a center ray misses, world-space contact data and touching motion.

`SweepHit` is the first time a moving box or sphere meets one static solid along a delta. `t` is the fraction of that delta, in `0..=1`. `normal` points out of the static solid. `point` is a representative contact position: an initial overlap uses the corresponding `Contact::point` where that query exists, which need not lie on the surface. `Aabb::sweep` and `Sphere::sweep_aabb` take an `Aabb`; `Aabb::sweep_obb` and `Sphere::sweep_obb` take an oriented box; `Aabb::sweep_sphere` takes a sphere; `Aabb::sweep_capsule` takes a capsule. The point and normal are in world space.

| Moving body | Static AABB | Static OBB | Static sphere | Static capsule |
| --- | --- | --- | --- | --- |
| AABB | `Aabb::sweep` | `Aabb::sweep_obb` | `Aabb::sweep_sphere` | `Aabb::sweep_capsule` |
| Sphere | `Sphere::sweep_aabb` | `Sphere::sweep_obb` | `Sphere::sweep_sphere` | No public sweep |
| Capsule | No public sweep | No public sweep | No public sweep | No public sweep |

All deltas translate a whole body in world coordinates; none of these queries
rotates that body or integrates velocity/gravity. The static shape is a snapshot
for the whole delta. The Blender tutorial's
[marked-level example](blender-to-macroquad.md#4-upload-once-move-and-draw)
shows dispatching an AABB sweep across imported solid marker shapes.

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

`examples/sweep_hit.rs` moves a box and a sphere into a wall; `examples/sweep_obb.rs` uses a yawed cover. A ray through the sphere's center can miss a grazing collision that a sphere sweep detects. One static-box sweep does not slide on its own: the examples keep the part of the delta that lies on the hit face. `slide_sphere` does that for a sphere against several boxes.

`Aabb::sweep_obb` keeps the full moving box axis-aligned and the static box at its authored rotation. It intersects the time intervals of all 15 separating axes, including edge/edge axes, so a tall body can hit an overhead corner even when its center ray misses. It does not rotate the moving box into a larger axis-aligned approximation or replace it with a sphere. The representative point lies in the shared region at impact; it is computed from clipped box edges. Equal entry times keep world X/Y/Z, then the box's local X/Y/Z, then the world/box cross products. This query provides one impact against one solid; ground state, sliding and step climbing remain application work.

```rust
use macroquad::math::{Aabb, Obb, Quat, Vec3, vec3};

let wall = Obb::new(Vec3::ZERO, vec3(0.05, 2.0, 3.0),
                    Quat::from_rotation_y(0.4));
let body = Aabb::from_center_size(vec3(-3.0, 0.0, 0.0), vec3(0.5, 1.8, 0.5));
let delta = vec3(6.0, 0.0, 0.0);
let hit = body.sweep_obb(delta, &wall).unwrap();
let at_impact = body.offset(delta * hit.t);
assert!(hit.normal.x < 0.0);
assert!(at_impact.center().x < 0.0);
```

`Aabb::sweep_sphere` tests the full box against a static sphere, including an edge or corner graze. It reuses `Sphere::sweep_aabb` with the opposite movement, then negates the normal and translates the contact point by `delta * t` into the caller's frame. The returned normal points from the sphere toward the moving box. For an initially separated pair, the point lies where the sphere meets the box at impact. For an initial overlap, it keeps the box point from `Sphere::contact_aabb`, which may lie inside or outside the sphere. The existing sphere sweep's tolerances and finite-precision behavior also apply.

```rust
use macroquad::math::{Aabb, Sphere, Vec3, vec3};

let body = Aabb::from_center_size(vec3(-3.0, 0.0, 0.0), vec3(0.2, 2.0, 0.5));
let rock = Sphere::new(Vec3::ZERO, 0.5);
let hit = body.sweep_sphere(vec3(6.0, 0.0, 0.0), &rock).unwrap();
assert!((hit.t - 0.4).abs() < 1e-4);
assert_eq!(hit.normal, -Vec3::X);
assert!((hit.point - vec3(-0.5, 0.0, 0.0)).length() < 1e-4);
```

`Aabb::sweep_capsule` tests the full box against a static capsule's cylinder and both rounded caps. It enumerates segment/box closest features and solves their continuous plane or quadratic contact times with `f64` intermediates; it does not substep or use the capsule's bounding box. A new impact has a shared world-space point and a normal out of the capsule toward the box. An initial overlap reverses `Capsule::contact_aabb`'s normal and keeps its box point, including that query's containment convention and floating-point limits. Penetration deeper than `1e-4` returns `t == 0`; a touch or shallower overlap only blocks inward motion greater than `1e-5`. New impacts also require that inward motion, so exact future tangencies miss. Coincident capsule endpoints delegate to `Aabb::sweep_sphere`. Zero radii and flat or point boxes are supported; invalid inputs miss. Finite extreme coordinates still have numerical limits. One sweep reports one impact; it does not add character sliding or ground state.

```rust
use macroquad::math::{Aabb, Capsule, Vec3, vec3};

let body = Aabb::from_center_size(vec3(-3.0, 0.0, 0.0), vec3(0.2, 2.0, 0.5));
let post = Capsule::new(vec3(0.0, -2.0, 0.0), vec3(0.0, 2.0, 0.0), 0.5);
let hit = body.sweep_capsule(vec3(6.0, 0.0, 0.0), &post).unwrap();
assert!((hit.t - 0.4).abs() < 1e-4);
assert_eq!(hit.normal, -Vec3::X);
assert!((hit.point.x + 0.5).abs() < 1e-4);
```

## Sphere slide

`slide_sphere(center, radius, delta, aabbs, obbs)` returns the new center. The sphere moves by `delta`, stops on an `Aabb` or `Obb`, and keeps the motion along that face. A sphere that only touches a floor still strafes, so the caller does not shrink the radius. A center that starts inside a box is pushed out along that box's shortest axis before the move: a world axis for an `Aabb`, one of the box's own axes for an `Obb`. There is no gravity here.

```rust
use macroquad::math::{slide_sphere, Aabb, vec3};

let floor = Aabb::from_center_size(vec3(0.0, -0.5, 0.0), vec3(4.0, 1.0, 4.0));
let moved = slide_sphere(vec3(0.0, 0.5, 0.0), 0.5, vec3(1.0, -0.25, 0.0), &[floor], &[]);
assert!((moved.x - 1.0).abs() < 1e-4);
assert!((moved.y - 0.5).abs() < 1e-4);
```

`slide_sphere_solids(center, radius, delta, aabbs, obbs, spheres, capsules)` is that move with static spheres and capsules added. `slide_sphere` is the same call with those two slices empty, and its arguments stay as they are. A sphere that only touches a rock still strafes. A vertical capsule stops a horizontal step on its round hull. Copying the capsule into an `Aabb` first makes that hull a square and blocks steps the capsule misses. A center that starts inside a sphere is pushed out from the other center, or along `Vec3::X` when the centers coincide. A center that lies on a capsule's inner segment is pushed out along the world axis most perpendicular to that segment, X before Y before Z.

```rust
use macroquad::math::{slide_sphere_solids, Capsule, vec3};

let column = Capsule::new(vec3(0.0, 0.0, 0.0), vec3(0.0, 2.0, 0.0), 0.45);
let moved = slide_sphere_solids(vec3(-2.0, 1.0, 0.0), 0.4, vec3(3.0, 0.0, 0.0), &[], &[], &[], &[column]);
assert!((moved.x + 0.85).abs() < 1e-3);
```

Keep triggers out of all four solid slices. A glTF collider's `trigger` flag
belongs to `GltfCollider`, not `GltfColliderShape`; the shape query alone cannot
distinguish a goal from a wall. After moving, test triggers separately with
`overlaps_sphere`. The [Blender tutorial](blender-to-macroquad.md) demonstrates
that split. The single-sphere helpers return a center only, with no contact
list, grounded state, velocity update, step climbing, or slope limit. Use the
explicit sweep and contact queries when a response also needs a hit normal.

### Rigid groups of spheres

For contact identity, `slide_spheres_solids_with_contacts` takes the same inputs
plus a synchronous callback and returns the identical translation. Each
`SphereSlideContact` carries `solid_kind`, the original per-kind `solid_index`,
`sample_index`, a world-space outward `normal`, and `PushOut` or `Sweep` phase.
The solver allocates no contact list; collect into caller-owned storage if needed.

Sweeps select earliest exact time, then the strongest opposing normal, then
sample order, AABB/OBB/sphere/capsule order, then slice index. Push-outs select
the largest squared correction, with the same sample/kind/index tie order.
Only applied initial corrections and selected sweep contacts are reported,
including blocked final probes; repeated events are possible. Zero motion
reports initial recovery only. Tangent touches alone and invalid/free moves
emit nothing. Events are solver observations, not a complete contact manifold
or a promise that every reported final probe was reached.

```rust
use macroquad::prelude::*;
let wall = Aabb::from_center_size(Vec3::ZERO, vec3(1., 8., 8.));
let requested = vec3(3., 0., 1.);
let up = Vec3::Z; // Caller-defined; the helper assumes no up direction.
let mut into_wall = 0.0_f32;
let moved = slide_spheres_solids_with_contacts(
    &[vec3(-2., 0., 0.), vec3(-3., 0., 0.)], 0.5, requested,
    &[wall], &[], &[], &[], |contact| {
        if contact.phase == SphereSlidePhase::Sweep && contact.normal.dot(up).abs() < 0.1 {
            into_wall = into_wall.max(-requested.dot(contact.normal));
        }
    },
);
assert!((moved - vec3(1., 0., 1.)).length() < 1e-4);
assert_eq!(into_wall, 3.0);
```

Use each normal with the caller's requested motion/up for scrape or stop
feedback. The callback does not change velocity, add friction or solve yaw.
`cargo test --test slide_contacts --test slide_contact_allocations` checks
identity, ordering, shared translation and allocation-free stack collection.


`slide_spheres_solids(centers, radius, delta, aabbs, obbs, spheres, capsules)`
takes world-space centers with one shared radius and returns one world-space
**translation**, including any initial push-out. Add it once to the owning
object's position, or add the same value to every center. All members advance
to the earliest hit by any member, then share the remaining slide. Their
separation stays fixed. Close collision times still use the earlier time;
only exactly equal times use the stronger opposing normal. The same ordering
applies to `slide_sphere_solids`. Contact planes are retained for the move so sliding
at a later contact cannot point back into an earlier one. Averaging independent
`slide_sphere_solids` results could move the leading member through a wall; it
does not produce this result.

```rust
use macroquad::math::{slide_spheres_solids, Aabb, vec3};

let floor = Aabb::from_center_size(vec3(0.0, -0.5, 0.0), vec3(20.0, 1.0, 20.0));
let wall = Aabb::from_center_size(vec3(3.5, 1.0, 0.0), vec3(1.0, 4.0, 20.0));
let mut hull = [vec3(-1.0, 0.5, 0.0), vec3(1.0, 0.5, 0.0)];
let separation = hull[1] - hull[0];
let translation = slide_spheres_solids(
    &hull, 0.5, vec3(4.0, -0.25, 2.0), &[floor, wall], &[], &[], &[],
);
for center in &mut hull {
    *center += translation;
}
assert!((translation - vec3(1.5, 0.0, 2.0)).length() < 1e-4);
assert!((hull[1] - hull[0] - separation).length() < 1e-4);
```

Touching a floor keeps its tangent, and initial penetration uses the same
per-solid push direction as `slide_sphere_solids`. Each push moves the whole
group and the next pass checks all members again. Members do not collide with
each other. Push-out and sliding use bounded passes; an impossible fit or
crowded initial overlap can remain unresolved, and complex contacts can stop
the motion conservatively. Pass only solids, and test triggers separately for
each translated sphere.

An empty center slice or a negative/non-finite radius permits the free `delta`.
A non-finite delta or any non-finite center returns zero translation instead.
Invalid static shapes are ignored as in the single-sphere helper. The caller
owns velocity, gravity and orientation. This function solves translation only;
yaw or other rotation needs a separate collision check before changing the
member offsets.

The [`slide_solids` example](../examples/README.md#slide_solids) switches between
the original single sphere and a two-sphere hull with Space.

## Physics

`RigidBody` in `math` and the prelude provides translation-only integration
and explicit pair collision response without a window or a physics world. It stores public `position`,
`linear_velocity`, `mass`, `linear_damping`, `mode` and `shape` fields.
`RigidBodyShape::Aabb(Aabb)` and `RigidBodyShape::Sphere(Sphere)` hold local
geometry: their corners/centers are offsets from the body's world-space
position. `world_shape()` translates a copy for the existing collision queries.
Boxes stay axis-aligned; local shapes may have an offset from the body origin.

`RigidBody::new(position, shape, mass)` checks its inputs and starts in
`RigidBodyMode::Dynamic` with zero velocity and damping. For positive mass,
`integrate(dt, gravity)` adds `gravity * dt` to velocity, multiplies it by
`exp(-linear_damping * dt)`, and advances position by the updated velocity
times `dt`. Gravity is a world-space acceleration chosen by the caller,
independent of mass magnitude and any up axis. `RigidBodyMode::Kinematic`
ignores gravity but still damps and integrates velocity. Zero mass, including
negative zero, makes either mode static: position **and velocity** stay fixed.

```rust
use macroquad::prelude::*;

let mut body = RigidBody::new(
    vec3(0.0, 3.0, 0.0),
    RigidBodyShape::Sphere(Sphere::new(Vec3::ZERO, 0.5)),
    1.0,
).unwrap();
body.linear_velocity = Vec3::X * 2.0;
body.integrate(0.5, -Vec3::Y * 8.0).unwrap();
assert_eq!(body.position, vec3(1.0, 1.0, 0.0));
let RigidBodyShape::Sphere(world_ball) = body.world_shape().unwrap() else {
    unreachable!();
};
assert_eq!(world_ball.center, body.position);

body.mass = 0.0;
let stopped = body;
body.integrate(1.0, -Vec3::Y * 8.0).unwrap();
assert_eq!(body, stopped);
```

Every integration call validates all inputs, including static/kinematic bodies
and zero time. Time, mass and damping must be finite and non-negative;
position, velocity and gravity must be finite. Local box corners must be finite
and ordered; sphere centers/radii must be finite, with non-negative radius.
Degenerate boxes and zero-radius spheres are accepted. The translated shape
must also stay finite. Invalid input or an unrepresentable result returns a
`RigidBodyError` without changing any field. Zero time is a successful no-op
after validation. The `world_shape()` query checks only position and shape,
including translated-coordinate overflow. Intermediate integration arithmetic
uses `f64`, rounds the updated velocity to `f32`, then computes position;
finite rounding/underflow is accepted. Subdividing time can change this
semi-implicit Euler trajectory.

Integration does not inspect solids or prevent tunneling. Call
`a.resolve_collision(&mut b, restitution)` separately to resolve one current
AABB/AABB, sphere/sphere or mixed pair. It returns the pre-correction
`Some(Contact)` for overlaps and touches, or `None` for separated shapes.
The contact normal points from `b` toward `a`; the contact point is diagnostic
and does not introduce torque. Shape offsets are translated through
`world_shape()` before querying.

Positive-mass dynamic bodies share the penetration correction in proportion
to inverse mass: a lighter body moves farther. A closing relative velocity
receives an impulse along the contact normal, with no friction or tangential
impulse. Resting and separating pairs receive no impulse, although an overlap
is still separated. Restitution must be finite and in `[0, 1]`: `0` removes
closing normal velocity and `1` gives an elastic normal bounce. Pass `0` when
no bounce is wanted.

Zero mass freezes all fields and its stored velocity is ignored by response.
A positive-mass kinematic body also receives no correction or impulse, but its
prescribed velocity participates in the relative velocity seen by a dynamic
body. Two immovable bodies still report their contact and stay unchanged.
This response policy does not change how `integrate` advances kinematic bodies.

```rust
use macroquad::prelude::*;

let mut floor = RigidBody::new(
    Vec3::ZERO,
    RigidBodyShape::Aabb(Aabb::from_min_max(
        vec3(-4.0, -1.0, -4.0), vec3(4.0, 0.0, 4.0),
    )),
    0.0,
).unwrap();
let mut ball = RigidBody::new(
    vec3(0.0, 0.25, 0.0),
    RigidBodyShape::Sphere(Sphere::new(Vec3::ZERO, 0.5)),
    2.0,
).unwrap();
ball.linear_velocity = vec3(3.0, -2.0, 0.0);
let before_floor = floor;
let contact = ball.resolve_collision(&mut floor, 0.5).unwrap().unwrap();
assert_eq!(contact.normal, Vec3::Y);
assert_eq!(ball.position.y, 0.5);
assert_eq!(ball.linear_velocity, vec3(3.0, 1.0, 0.0));
assert_eq!(floor, before_floor);
```

`sphere_body.resolve_static_obb(&box_collider, restitution)` uses a separate
world-space `Obb` collider and the same response rules, with a stationary
immovable box. It supports sphere bodies only, preserving the existing
`RigidBodyShape` variants. Select solid colliders explicitly; triggers should
remain overlap queries.

```rust
use macroquad::prelude::*;

let cover = Obb::new(Vec3::ZERO, Vec3::ONE, Quat::IDENTITY);
let mut ball = RigidBody::new(
    vec3(1.25, 0.0, 0.0),
    RigidBodyShape::Sphere(Sphere::new(Vec3::ZERO, 0.5)),
    1.0,
).unwrap();
ball.linear_velocity = vec3(-2.0, 0.0, 3.0);
let contact = ball.resolve_static_obb(&cover, 0.0).unwrap().unwrap();
assert_eq!(contact.normal, Vec3::X); // box toward sphere, before correction
assert_eq!(ball.position.x, 1.5);
assert_eq!(ball.linear_velocity, vec3(0.0, 0.0, 3.0)); // tangent retained
```

Response checks both bodies, including mass, damping, velocity and geometry,
even for separated or immovable pairs. Errors leave every field of both
bodies unchanged. The separate `CollisionResponseError` preserves the existing
`RigidBodyError` variants and reports invalid restitution, invalid collider,
unsupported shape and unusable contact arithmetic. Very large or tiny geometry
can return `NumericalFailure` when the existing contact query's `f32`
intermediates would overflow or lose a nonzero squared distance/radius to
underflow. The existing contact queries themselves are unchanged. Finite
rounding and underflow of the resulting body update are accepted, so a very
small correction at a large coordinate may round away. See the methods'
rustdoc for the numerical checks.

The caller chooses integration order, candidate pairs and repeated passes.
For many bodies, index their current world AABBs in `UniformGrid`, query
candidate indices, and visit each unordered pair once per pass. Reinsert
changed bounds or rebuild before querying the next updated configuration:
the grid stores snapshots, and separation can create contacts with other
bodies. It supplies candidates only; response still queries the actual shapes.
One discrete pair step cannot prevent tunneling or guarantee a settled stack.
Rotation, angular response, continuous solving and islands remain separate.

A typical application tick integrates its bodies, updates candidate bounds,
resolves the selected solid pairs, then draws their corrected positions.
Keep trigger queries separate. Choose a stable pair order for reproducibility;
multiple discrete responses can depend on that order. Each call is transactional,
but a whole tick is not: if a later response fails, successful earlier integration
or response calls remain applied. A game that needs tick-wide rollback must
save and restore its own body state. Substeps can reduce travel per tick, but
they do not turn this API into continuous collision detection.

A game can instead keep its own velocity and call `slide_sphere` for a sphere
against boxes, or `slide_sphere_solids` when a solid is a sphere or a capsule.
These slide helpers retain tangential motion and do not apply mass or gravity.
`Contact` describes an overlap that already exists; it does not move either
shape. The following manual movement recipe remains useful when solids must
constrain each step.

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

`CharacterController::move_body` takes a world-space `Aabb`, velocity, elapsed
time and `CharacterSolids`, and returns a `CharacterMotion`: the moved body,
clipped velocity, `on_ground` and optional world-space `ground_normal`. The body
keeps its full dimensions and world-axis alignment. Static AABBs, OBBs,
spheres and capsules use the completed full-body sweeps; the moving shape is
always an AABB. All APIs are in `math` and the prelude, without a window.

The caller supplies gravity and rebuilds its desired movement each step.
`CharacterController` contains only settings: `up` (normalized internally),
`max_slope_angle` in radians, `ground_snap_distance` and `max_iterations`.
The defaults are +Y, 45 degrees, 0.1 world units and eight passes. A non-Y up
direction does not rotate the axis-aligned body. Invalid settings, bounds,
velocity or elapsed time leave the body unchanged and ungrounded.

```rust
use macroquad::math::{Aabb, CharacterController, CharacterMotion, CharacterSolids, Vec3, vec3};

fn step_player(
    previous: CharacterMotion, horizontal_wish: Vec3, jump: bool,
    solids: CharacterSolids<'_>, dt: f32,
) -> CharacterMotion {
    // This sample uses Y-up. Rebuild horizontal movement and preserve falling
    // velocity. A ramp's previous upward tangent is not a new jump request.
    let mut velocity = horizontal_wish;
    velocity.y = if previous.on_ground { 0.0 } else { previous.velocity.y };
    if previous.on_ground && jump { velocity.y = 5.5; }
    velocity.y -= 9.8 * dt;
    CharacterController::default().move_body(previous.body, velocity, dt, solids)
}

let floor = Aabb::from_center_size(vec3(0.0, -0.5, 0.0), vec3(20.0, 1.0, 20.0));
let previous = CharacterMotion {
    body: Aabb::from_center_size(vec3(0.0, 0.9, 0.0), vec3(0.6, 1.8, 0.6)),
    velocity: Vec3::ZERO, on_ground: true, ground_normal: Some(Vec3::Y),
};
let solids = CharacterSolids { aabbs: &[floor], ..Default::default() };
let moved = step_player(previous, Vec3::X * 2.0, false, solids, 1.0 / 60.0);
assert!(moved.on_ground);
assert!(moved.body.center().x > 0.0);
```

Movement projects the untraveled displacement along blocking normals and keeps
previously encountered planes, so floor strafing survives a wall contact and
corners cannot reintroduce motion into an earlier wall. Walkable slopes may
turn horizontal motion into an upward tangent. Steeper upward faces block that
climb; falling can slide downhill. Vertical faces are never ground.

Grounding checks the **final** body, including after walking off a ledge. A
downward probe stops at the first solid and snaps only onto a walkable face
within `ground_snap_distance`; it cannot pass through an unwalkable slope to
find a floor below. Zero snap distance still recognizes touching support. A
positive input velocity along `up` skips snapping and grounding so jumping
detaches immediately. Consequently, build the desired grounded movement
before each call instead of treating a previous ramp tangent as a jump. Set
snap distance to zero while falling when only ordinary landings should attach.

Construct `CharacterSolids` from **solid-only** slices. Imported triggers must
be excluded when collecting those slices, then queried after movement with
overlap queries. Initial overlaps are recovered deepest first using a bounded
number of passes; an unresolved overlap leaves the recovered body ungrounded
and prevents an unchecked movement. Exhausted slide passes discard the
unresolved displacement. The existing sweep tolerances still apply; this is
not a general penetration or rigid-body solver.

For stairs, opt into `move_body_with_step` with a maximum step height in world
units. It returns the resolved motion together with step diagnostics; existing
`move_body` calls remain available with the same settings literals.

```rust
use macroquad::prelude::*;
let floor = Aabb::from_center_size(vec3(0., -0.5, 0.), vec3(20., 1., 4.));
let stair = Aabb::from_center_size(vec3(1., 0.1, 0.), vec3(1., 0.2, 4.));
let body = Aabb::from_center_size(vec3(0., 0.9, 0.), vec3(0.6, 1.8, 0.6));
let solids = CharacterSolids { aabbs: &[floor, stair], ..Default::default() };
let result = CharacterController::default()
    .move_body_with_step(body, vec3(1., -0.1, 0.), 0.5, solids, 0.25);
assert!(result.motion.on_ground);
assert!((result.motion.body.min.y - 0.2).abs() < 1e-4);
```

The candidate path sweeps the complete body upward, forward and down; a low
ceiling cannot be bypassed. Only grounded, non-jumping movement can step.
Landing still requires a walkable normal and an improvement over ordinary
sliding. An upward step must land higher; it cannot vault a barrier onto the
floor behind it. Grounded descending movement can attach to a lower tread
within the step height measured from its initial support; a deeper ledge
leaves the ordinary falling motion intact. This tests a single
translation path per call, so several risers crossed in one long tick can
still block. Use an appropriately bounded movement tick for your stair treads.
Zero step height disables the extra path. Inspect the diagnostic outcome and
contact normals/points when a step fails, rather than raising the body after
an overlap test. Gravity, moving platforms and the choice of body dimensions
remain in the caller.

`examples/character_controller.rs` is a self-contained playable sample with a
thin wall, a low ceiling, ramps, rounded solids, five gold stair treads and a
nonblocking trigger. T toggles a 0.25-unit step height; its HUD displays the
step diagnostics. The controller has no capsule player, platform transport or
rigid-body impulse coupling. Existing `slide_sphere_solids` and `Capsule` contact
queries remain useful for prototypes; capsule overlap tests alone do not
provide continuous capsule movement.

## Drawing

Set a `Camera3D` before any of these calls. The samples use `up: Vec3::Y`.

`draw_aabb(aabb, color)` is `draw_cube_wires(aabb.center(), aabb.size(), color)`. An inverted box still outlines that span; call `Aabb::normalized` before an overlap or ray query, which miss on the inverted box. `draw_sphere` / `draw_sphere_wires` take a center and a radius, the same fields as `Sphere`. `draw_ray(ray, t, color)` is the segment from the origin to `ray.point_at(t)`; a negative `t` extends behind the origin. `draw_triangle_wires` is the three edges. `draw_obb` is the twelve edges. `half_extents` is half of `draw_cube`'s full size. A negative half extent draws nothing, the same as the overlap queries. A zero rotation is drawn as identity; those queries miss. `draw_capsule_wires` draws `draw_sphere_wires` at `a` and `b` and four lines `radius` out from the inner segment. A negative radius draws nothing.

`draw_plane(center, size, texture, color)` is an older helper: a quad in the XZ plane. `size` is the half-extent on X and Z, and the quad can take a texture. `draw_plane_wires(plane, center, size, color)` follows `plane.normal`. `size` is the full side length. `center` is projected onto the plane. A non-unit normal is normalized and `distance` is scaled by the same factor.

```rust,no_run
use macroquad::prelude::*;

let floor = Aabb::from_center_size(vec3(0.0, -0.05, 0.0), vec3(8.0, 0.1, 8.0));
let ground = Plane::new(Vec3::Y, 0.0);
draw_aabb(floor, DARKGREEN);
draw_plane_wires(ground, Vec3::ZERO, 4.0, GREEN);
```

`examples/debug_draw.rs` draws one of each wireframe. Call these after `set_camera`.

## Feature flags

The collision types above are always compiled. Parsing and drawing a glTF file require the `gltf` feature, which is not a default feature. See the [Blender tutorial](blender-to-macroquad.md).

## Heightfield terrain

`Heightfield::new(columns, rows, heights, scale, origin)` owns a regular row-major
height grid with default Y up. `with_up(..., up)` normalizes an explicit up vector.
The column axis projects +X onto its horizontal plane (projecting +Z instead when
`abs(up.x) > 0.9` after normalization); the row axis is column cross up.
`horizontal_axes()` exposes both. `scale.x`/`.z` are positive horizontal spacings,
`.y` is a signed height multiplier, and origin is the zero-height first sample.

```rust
use macroquad::prelude::*;
let terrain = Heightfield::with_up(2, 2, vec![0.0, 1.0, 1.0, 2.0],
    Vec3::ONE, Vec3::ZERO, Vec3::Z).unwrap();
let height = terrain.sample_height(0.25, 0.5).unwrap();
let (u, v) = terrain.horizontal_axes();
let surface = terrain.origin() + u * 0.25 + v * 0.5 + terrain.up() * height;
assert!(terrain.overlaps_aabb(&Aabb::from_center_size(surface, Vec3::splat(0.01))));
assert!(terrain.ray_intersect(&Ray::new(surface + Vec3::Z, -Vec3::Z)).is_some());
```

Sampling takes horizontal distances relative to origin, includes the outer edges
rounded to f32, and returns scaled height along up. It uses the same two planar
triangles per cell as collision, split from (column+1,row) to (column,row+1),
rather than bilinear interpolation. Outside/non-finite samples miss. Construction
rejects invalid dimensions, inputs or unrepresentable extents/vertices.
`mesh()` exposes the immutable world-space triangles; ray indices are
`2 * (row * (columns-1) + column) + half`, with half zero below the diagonal.
Ray units, exact ties and numerical limits follow `TriangleMesh`; AABB overlaps
include surface touches but do not treat terrain as a solid volume below ground.
No controller response or acceleration structure is added.


### Static octree candidates

`Octree` is a build-once alternative to the mutable `UniformGrid` for static
world-space AABB snapshots. Caller `u32` indices remain application-owned.
Construct a new tree when objects move. `new` rejects any non-finite/inverted
box; valid repeated indices use the last bounds. Empty input is valid.

```rust
use macroquad::prelude::*;
let wall = Aabb::from_center_size(Vec3::X * 4.0, Vec3::ONE);
let tree = Octree::new([(12, wall)]).unwrap();
assert_eq!(tree.query_aabb(&wall), vec![12]);
assert_eq!(tree.query_ray(&Ray::new(Vec3::ZERO, Vec3::X * 2.0), 2.0), vec![12]);
```

Boxes crossing a split plane stay in the parent; each entry is stored once.
`with_max_depth` clamps the subdivision limit to 16; zero uses one linear leaf.
Degenerate clusters stop splitting, and leaves can exceed the eight-entry target.
Queries include touches and return sorted unique indices, not distance order or
contacts. Ray limits use `origin + direction * t`, allow positive infinity and
retain nonzero subnormal directions. The grid and octree share double-precision
slab predicates and their conservative rounding limits at extreme scales.
Check the actual shapes after selecting candidates. This does not add a physics
world, mesh acceleration or a measured performance claim.


### Distance joints

`DistanceJoint::new(rest_length)` creates a caller-stepped constraint between
`RigidBody::position` origins, available in math/the prelude without a window.
After integrating bodies, call `joint.solve(&mut a, &mut b)`. Both public
correction fractions default to 0.2 and must be in `0..=1`. Each call removes
that fraction of distance error and relative radial speed, for compression
as well as extension, without changing tangential speed or adding velocity
from the position correction. Fractions apply per call, independent of time.

```rust
use macroquad::prelude::*;
let shape = RigidBodyShape::Sphere(Sphere::new(Vec3::ZERO, 0.1));
let mut anchor = RigidBody::new(Vec3::ZERO, shape, 0.0).unwrap();
let mut bob = RigidBody::new(Vec3::Y * 4.0, shape, 1.0).unwrap();
let joint = DistanceJoint::new(2.0).unwrap();
joint.solve(&mut anchor, &mut bob).unwrap();
assert!((bob.position.y - 3.6).abs() < 1e-6);
```

Dynamic bodies share correction by inverse mass. Static bodies stay fixed and
their stored velocity is ignored; positive-mass kinematic anchors stay unchanged
but contribute prescribed velocity. Immovable pairs are validated no-ops.
Coincident origins with positive rest length use world +X from the first to the
second argument; this ambiguous case depends on argument order. Zero rest length
corrects the full displacement and all relative velocity, including at coincidence.

Settings and both bodies are validated on every call, even with zero correction.
Any error leaves both bodies unchanged, including if only the second proposed
world shape overflows. Arithmetic uses f64 and accepts finite f32 rounding and
underflow, which can leave residual error. Refresh spatial bounds after movement.
Choose a fixed application step and a consistent pair order before tuning
fractions. If you call `solve` twice in a step, the default position fraction
removes 36% of an isolated error (1 - 0.8 * 0.8), rather than 20%. Changing
render FPS should not silently change the number of solver calls. Rebuild an
octree, or update grid snapshots, after corrections before the next query.

No local attachment offsets, angular joints, automatic collision filtering,
continuous solving or world ownership are supplied. Pair order and repetition
count affect the result; networks have no convergence or stability guarantee.

### Combine integration, contacts and one joint

Run `cargo run --example rigid_body_joint` for a small Y-up scene with a free
sphere, one anchored sphere and a primitive floor. The
[shared scene](../examples/support/rigid_body_joint_scene.rs) composes only the
public checked APIs; it is application code, not a physics world. P or Space
pauses, R resets, O switches the joint/contact order and resets, K switches the
anchor mode and resets, and T runs the deterministic replay checks. Reset and
mode/order changes preserve the pause state. The
[example index](../examples/README.md#rigid_body_joint) includes WebGL setup.

The example accumulates render-frame time into fixed 1/120-second steps. It
accepts at most 0.1 seconds from one frame and performs at most 12 steps per
frame, dropping excess elapsed time rather than catching up without a bound.
Pause, resume and reset clear the accumulator and skip their transition frame,
so paused time cannot become a large integration step. These are example clock
choices: the library does not supply a scheduler or a timestep-independent
solver. Changing the fixed step or the number of joint calls changes trajectory
and joint correction per simulated second.

Each step integrates the bodies, then calls the joint once and visits contacts
in a fixed order: free sphere/floor, bob/floor, free sphere/bob. O moves the
single joint call after those contacts. Restitution is 0.2. The joint uses
position correction 0.8 and velocity correction 0.2 **per call**; these are
fractions, not rates multiplied by `dt`. The anchor is excluded from contact
pairs. This explicit pair list needs no broadphase; larger scenes must maintain
their own candidate bounds and ordering.

The default anchor has zero mass and remains fixed, with any stored velocity
ignored by response. K selects a positive-mass `Kinematic` anchor: the caller
prescribes its velocity and integrates it with zero damping, while contacts and
the joint cannot move it. Its prescribed velocity affects the bob's radial
correction. Setting zero mass on a kinematic body would freeze it too; mass zero
does not describe a moving anchor.

The HUD displays the caller-computed
`abs(distance(anchor.position, bob.position) - rest_length)` after the complete
step, alongside contacts and replay check totals. An
isolated probe also compares both orders from the same starting state: contact
correction after the joint can increase distance error; joint correction after
contacts can pull the bob into the floor again. A smaller distance residual is
therefore not proof that contacts are satisfied. In the probe, joint then
contacts leaves 0.20 distance error and no overlap; contacts then joint leaves
0.04 error and 0.16 overlap. The scene contains only one anchored pair and an
unconnected body; it makes no joint-network stability or continuous-collision
guarantee.

Each checked library call leaves its own inputs unchanged on error. The example
also stages a whole step in a copy and commits it only when every operation
succeeds. An error pauses the scene, clears pending time and displays the error;
reset reconstructs the initial valid state. The headless tests exercise this
policy, the bounded clock, pause/reset transitions and the ordering probe
without opening a window.
