# 3D guide

This fork adds 3D queries to `macroquad::math`. They are also in the prelude (`use macroquad::prelude::*`). The same types are in `macroquad::math` for a test or a tool that must not open a window.

Nothing here assumes an up axis. `Camera3D::up` stays caller-defined. The examples in this repository use **Y-up**, which matches a glTF file exported from Blender. Sizes are unitless. A glTF scene is in meters, and `get_frame_time` is in seconds, so a gravity of `(0, -9.8, 0)` is meters per second squared when the level is left in those units.

`draw_cube` / `draw_cube_wires` take a center and a **full** size. `Aabb::from_center_size` uses that same pair. `Obb::half_extents` is half of that full size. `Capsule` `a` and `b` are the inner segment, so the hull extends `radius` past each end.

There is no rigid-body world and no character controller in the library yet. `Contact`, `SweepHit`, `slide_sphere`, and `slide_sphere_solids` are queries a game uses to write those itself. The physics and character sections below are application code. The step-by-step [Blender tutorial](blender-to-macroquad.md) covers exporting, loading, drawing, and moving through a marked level. The [example index](../examples/README.md) lists runnable samples.

Imported levels continue in [Retained geometry](#retained-geometry), [Resource diagnostics](#resource-diagnostics), [Retiring unused render resources](#retiring-unused-render-resources), [Pollable glTF construction](#pollable-gltf-construction), [Loading and materials](#loading-and-materials), and [Opt-in glTF material passes](#opt-in-gltf-material-passes). That last section includes selected-part submission, [optional basic lighting](#optional-basic-lighting), and shaded normal/roughness maps. [Replacing an imported node pose](#replacing-an-imported-node-pose), [Editing an imported hierarchy](#editing-an-imported-hierarchy), [CPU morph data and evaluation](#cpu-morph-data-and-evaluation), and [Drawing independent morph instances](#drawing-independent-morph-instances) cover moving a baked node and drawing shape keys. Collision queries start at [Ray hits](#ray-hits). Gravity and a capsule standing on a floor are the application samples in [Physics](#physics) and [Character movement](#character-movement). Mouse look uses [Logical and physical input](#logical-and-physical-input) and [Pointer capture](#pointer-capture). Browser density and frame-rate caps are in [Browser canvas budgets](#browser-canvas-budgets).

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
deforming draw instances are described below. Shader-based morphing, skinning,
animation, environment lighting, shadows and transmission remain roadmap work.

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
methods. Spawn markers, morphs, skinning and clips stay where they were loaded.
`collider_shape(&scene, collider)` returns one imported collider in the current
worlds. The list on `GltfScene` stays at rest, and `trigger` is still that
stored flag, so a slide keeps taking only the solid shapes. The query applies
`posed_world * inverse(rest_world)` to the imported shape. A quarter turn stays
an `Aabb`; any other rotation is an `Obb`. A sphere's radius grows by the
delta's largest stretch. A capsule's endpoints follow the delta, and its radius
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
keep deforming draw geometry and its bounds together. Tangent deltas, weight
clips and skinning remain separate work.

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
Shader-based deformation, tangent morphs,
weight clips and skinning are separate work.

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

`SweepHit` is the first time a moving box or sphere meets one static box along a delta. `t` is the fraction of that delta, in `0..=1`. `normal` points out of the static box. `point` is a representative contact position: an initial overlap uses the corresponding `Contact::point`, which need not lie on the surface. `Aabb::sweep` and `Sphere::sweep_aabb` take an `Aabb`; `Sphere::sweep_obb` takes an oriented box and returns the point and normal in world space.

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
that split. Slide helpers return a center only, with no contact list, grounded
state, velocity update, step climbing, or slope limit. Use the explicit sweep
and contact queries when a response also needs a hit normal.

## Physics

The library does not integrate velocity, apply gravity, or solve contacts. There is no `RigidBody` type. A game keeps the velocity and calls `slide_sphere` for a sphere against boxes, or `slide_sphere_solids` when a solid is a sphere or a capsule. Gravity is added to the velocity first. The slide keeps the part of the step that lies on a face, so a resting sphere still strafes and a wall removes only the component into the wall. Restitution and mass are not applied. `Contact` describes an overlap that already exists; it does not move either shape.

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

There is no character controller. `on_ground`, slope limits, and step offsets are not library fields. `Capsule` is a possible body shape, but no current sweep or slide moves a capsule. `slide_sphere_solids` moves a sphere against a static capsule. The moving-shape sweep queries are `Aabb::sweep`, `Sphere::sweep_aabb`, and `Sphere::sweep_obb`.

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

Wish velocity is still the caller's. A capsule moved with `offset` and checked
only at the endpoint can tunnel through a thin wall. Rejecting every overlap
also blocks a resting touch on the floor. The sample ground test reports one
contact; it does not supply a robust capsule movement solver. A sphere-based
prototype can use `slide_sphere_solids` today. Ground state, stairs, ceilings,
slopes, and a continuous capsule controller remain application responsibilities.

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
