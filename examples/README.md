# Examples added on this fork

The other files in this directory are the upstream macroquad samples. This
page lists this fork's collision, camera, glTF, retained geometry, material
regression, and video examples. The `gltf` feature enables the glTF loaders;
`video` independently enables video playback.

| Area | Examples | What to inspect |
| --- | --- | --- |
| Shapes and rays | `aabb_overlap`, `sphere_overlap`, `ray_cast`, `plane_hit`, `triangle_hit`, `obb_overlap`, `capsule_overlap` | Inclusive overlaps, hit normals, shape sizes, nearest hits |
| Motion queries | `sweep_hit`, `sweep_obb`, `slide_solids` | Box/sphere against an AABB, sphere against oriented cover, sphere slide against all four solid shapes; additional full-AABB body sweep fixtures are in the CPU tests |
| Character movement | `character_controller` | Caller-owned AABB, solid-only sweeps, jumping, ceiling/wall sliding, ground snap, slope limits and optional stair stepping with diagnostics |
| Camera and debugging | `picking_ray`, `frustum_cull`, `debug_draw` | Screen picking, conservative frustum queries, shape wireframes |
| Input coordinates | `input_coordinates` | Logical/physical mouse and touch positions, relative units, resize and density changes |
| Imported levels | `gltf_textured`, `gltf_retained`, `gltf_pollable`, `gltf_materials`, `gltf_material_check`, `gltf_shading`, `gltf_pose_check`, `gltf_morph_check`, `gltf_animation_check` | Immediate/retained GLB draw, staged loading, opt-in shading, pose/morph instances and caller-timed transform/weight clip pixel fixtures |
| Renderer behavior | `retained_mesh`, `material_depth_check`, `font_atlas_lifetime` | Immutable buffers and draw order, independent depth writes/testing, queued samplers, queued text across atlas growth and font drop |
| Video | `video_texture` (`video` feature) | Play/pause, synchronized soundtrack, and a reusable frame texture |
| Input | `cursor_capture` | Pointer-capture state, browser denial and unlock, desktop grab |
| Browser host | `canvas_host` | Density, framebuffer budgets, and a requested frame-rate cap |
| Rounded panels | `rounded_panels`, `rounded_panels_check` | Filled radius-clamped panels and shared native/WebGL single-alpha-coverage regression; see the [UI and 2D guide](../docs/ui-2d-guide.md#draw-a-rounded-panel) |
| Existing UI/2D samples | `camera`, `camera_transformations`, `texture`, `ui`, `ui_skins`, `platformer` | World/HUD cameras, texture drawing, widgets/skins, tile maps and platformer collisions; see the [UI and 2D guide](../docs/ui-2d-guide.md) for current limitations |

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
cargo run --example slide_solids
cargo run --example character_controller
cargo run --example picking_ray
cargo run --example frustum_cull
cargo run --example debug_draw
cargo run --example input_coordinates
cargo run --features gltf --example gltf_textured
cargo run --example material_depth_check
cargo run --example retained_mesh
cargo run --example font_atlas_lifetime
cargo run --features gltf --example gltf_retained
cargo run --features gltf --example gltf_pollable
cargo run --features gltf --example gltf_materials
cargo run --features gltf --example gltf_material_check
cargo run --features gltf --example gltf_shading
cargo run --features gltf --example gltf_pose_check
cargo run --features gltf --example gltf_morph_check
cargo run --features gltf --example gltf_animation_check
cargo run --features video --example video_texture -- path/to/clip.mp4
cargo run --example cursor_capture
cargo run --example canvas_host
cargo run --example rounded_panels
cargo run --example rounded_panels_check
```

These commands run from the fork root and open a window. To compile a sample
without opening a window, use `cargo build --example <name>` with the same
feature flags and omit runtime arguments after `--`. Pure collision and scene-loader tests can run without graphics;
rendering and texture uploads need a graphics context.

`rounded_panels_check` exports `rounded_panels_status` (0 pending, 1 passed,
2 panic), `rounded_panels_pixels` and `rounded_panels_dpi_milli` for browser
inspection. Build with
`cargo build --target wasm32-unknown-unknown --example rounded_panels_check`,
place the resulting WASM beside
`rounded_panels_check.html`, and serve the repository root. It checks pixels
on an offscreen target and then draws a translucent panel on the default
window camera; no assets or optional features are required.

## `character_controller`

Arrows/WASD move a box character; Space jumps, T toggles stepping, P pauses and R resets. The
caller supplies gravity, velocity and the world-space AABB to
`CharacterController::move_body_with_step`. A thin wall, low ceiling, two differently
sloped OBB ramps, a sphere and a capsule exercise all four static solid paths.
The final support normal is drawn in red. The blue trigger is queried after
movement and omitted from `CharacterSolids`, so it never blocks the body.
Five gold risers exercise ascent/descent with a 0.25-unit step height. The HUD
reports the step outcome and contacts, including rejected clearance/landing.
Moving platforms remain separate work. See
[Character movement](../docs/3d-guide.md#character-movement) for the API flow.

## `font_atlas_lifetime`

Runs a self-contained GPU pixel check for text queued across multiple font-atlas
replacements and a font dropped before flushing. It also checks a surviving font
after resource collection and canceled draws. The example requests high-DPI
rendering; run its WASM build at browser densities 1 and 2 to exercise real
density-scaled glyphs. It uses only the bundled ProggyClean font. The native
integration test shares the same fixture.

```sh
cargo build --target wasm32-unknown-unknown --example font_atlas_lifetime
```

## `input_coordinates`

Displays the public logical and physical mouse/touch APIs together. Logical
coordinates match 2D drawing and HUD hitboxes; physical coordinates use
framebuffer pixels. Mouse deltas are positive right/down. The example also
shows the legacy inverted normalized delta. Move, resize, change display
density, and touch the window to compare values. `examples/input_coordinates.html`
is a small browser fixture host for the WASM build, with JSON state for automated
checks. It needs the matching bundle and WASM served over HTTP.

```sh
cargo build --target wasm32-unknown-unknown --example input_coordinates
```

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

This example moves a sphere, not an AABB player body. The additive
`Aabb::sweep_obb`, `sweep_sphere` and `sweep_capsule` methods sweep the full body;
their synthetic headless fixtures live in `tests/aabb_obb_sweep.rs`,
`tests/aabb_sphere_sweep.rs` and `tests/aabb_capsule_sweep.rs`. See the
[motion guide](../docs/3d-guide.md) for query semantics and caller-owned response.

## `slide_solids`

Moves a sphere with the arrow keys. `slide_sphere_solids` slides it on a floor `Aabb`, past a `Sphere` rock, and around a vertical `Capsule` column. A step that only touches a solid keeps going. The sphere turns red when the step is shortened. Escape quits.

## `picking_ray`

Moves the mouse. `Camera3D::screen_to_ray` unprojects the cursor with the same view-projection the example passes to `set_camera`. `Ray::closest_aabb` marks the nearest box. The camera is Y-up. Escape quits.

The input point is top-left, y-down pixels of the target surface. The viewport
controls that rectangle; it does not automatically change the camera's aspect.
Perspective rays start at the camera and orthographic rays on the near plane.

## `frustum_cull`

Looks at a grid of boxes from the side. The yellow wireframe is a second `Camera3D`. `Camera3D::frustum` builds a `Frustum`, and `intersects_aabb` turns the boxes it meets green. Left and right yaw that camera. Space hides boxes outside it. Escape quits.

Use `intersects_aabb` for culling, not `contains_aabb`, which requires the whole
box to be inside. Plane-based intersection tests can retain a box outside a
frustum corner; this conservative result is intentional.

## `debug_draw`

A static scene of one of each wireframe: `draw_aabb`, `draw_obb`, `draw_capsule_wires`, `draw_triangle_wires`, `draw_plane_wires`, and `draw_ray`. `draw_plane_wires` takes the full side length and projects its center onto the plane. `draw_plane` is still the XZ quad. Escape quits.

## `gltf_textured`

Requires `--features gltf`. With no argument it draws `assets/textured_quad.glb`, a checkerboard quad. An argument is a `.glb` path passed to `load_file`, then `GltfModel::from_bytes`. The camera orbits the model's bounds with `up: Vec3::Y`. `model.draw()` submits the textured chunks. The on-screen line is `model.stats`.

`gltf_textured` still builds without the feature: that binary prints a message and exits. It does not parse a file unless `gltf` is enabled. The sample uses the default unlit material; loaded material metadata does not automatically provide lighting, alpha masking, or transparency sorting.

## `retained_mesh`

Uploads one small mesh once and draws it repeatedly with different transforms,
mixed with ordinary immediate drawing. Handle clones share immutable geometry;
this is separate draw submission, not hardware instancing. Geometry uploads
once; each call still submits a draw with its current camera, material, and
transform. Escape quits.

## `gltf_retained`

Requires `--features gltf`. Converts the small checkerboard GLB to a
`RetainedGltfModel`, explicitly releases CPU geometry, and draws instances with
shared buffers and textures. One instance is reflected. Transformed bounds
support frustum culling, and the display reports submitted chunks. It opts into
resource diagnostics before loading: the console prints one deduplicated model
report, source image dimensions and initial upload counts; the HUD shows the
previous frame's backend draws and retained/streaming upload bytes. Text uses
streaming geometry, so its uploads remain visible even when retained uploads
stay zero. These estimates and operation counts are not driver/process memory
or a frame-time benchmark. The
adapter preserves current-material drawing; custom lighting and front-face
selection use `GltfModelTransform`. The default material is unlit and does not
cull, so it also draws the reflected instance. See the retained geometry section of the
[3D guide](../docs/3d-guide.md) for that shader contract.

This example also builds without `gltf`, in which case it prints a feature
reminder and exits. Escape quits the feature-enabled sample.

## `gltf_pollable`

Requires `--features gltf`. With no argument it loads the small checkerboard;
an optional GLB path is read with `load_file` first. It starts paused so the
individual stages are easy to inspect. Space toggles continuous construction,
N advances one unit, C cancels, R restarts, and Escape exits. A moving indicator
continues while loading is paused. The example calls `GltfModelLoader::poll`
once per frame, then gives the completed model to `RetainedGltfModelLoader` for
one retained chunk upload per poll. The parsed geometry and textures are reused.

The display shows counts for the current stage, not an overall percentage.
Parsing, converting one primitive, and decoding one image remain synchronous;
an individual unit can stall a frame. Source-image limits apply before decode,
and an independent upload cap reduces accepted images. Cancellation, errors
and dropped handles release partial ownership; ordinary frames collect unused
resources, and restart also uses the explicit retirement boundary. This is
render-thread loading, with no worker decoding or strict frame-time guarantee.
The example builds without `gltf` and prints a feature reminder.

## `gltf_materials`

Requires `--features gltf`. Loads the embedded checkerboard, or a GLB path given
on the command line, and prints structured unsupported-material warnings.
It draws two retained instances in one `GltfMaterialRenderer` collection so
transparent chunks sort across both instances. The opt-in renderer applies
opaque/mask/blend alpha, depth writes, sidedness and sRGB base/emissive conversion.
It enables optional hemisphere/directional diffuse lighting and exposes its
world-space direction and linear light colors as `GltfLighting` values. L toggles
lighting to compare with the base/emissive preview. The second instance has a
reflected, nonuniform scale. Imported normals use the composed inverse transpose,
and textured emission remains independent of lighting. This sample calls
`submit_retained`, so it does not sample normal or metallic-roughness maps.
`gltf_shading` does. Environment lighting, occlusion, refraction and shadows
are absent. `KHR_materials_unlit` uses base
color only in both modes. Escape exits.

The default output encodes RGB before destination-space blending. Use linear
output into a linear target with your own final conversion for linear
compositing. Center sorting cannot resolve intersecting transparent triangles.
This sample submits each model whole. To omit a chunk or move one without
cloning the model, borrow `model.parts` and call `submit_retained_part` with
an extra affine transform. Positions are already baked, so a spin around a
chosen point is `T(pivot) * R * T(-pivot)`. See the
[material passes guide](../docs/3d-guide.md#opt-in-gltf-material-passes).

## `gltf_shading`

Requires `--features gltf`. Draws one procedural quad with `submit_shaded`.
The left half is a rough dielectric with a flat normal. The right half is a
rough metal whose normal map points toward +X. The directional light orbits.
Maps are linear: normal-map XY is the tangent direction after the scale, green
is roughness and blue is metallic. Escape exits. Without `gltf`, it prints a
feature reminder and exits.

## `gltf_material_check`

Requires `--features gltf`. It runs the same pixel assertions as
`tests/gltf_material.rs`, including selected-part filtering, independent CPU
and retained transforms, reflected parents, lighting, and mixed whole-model
transparency. A passing run prints `GLTF_MATERIAL_FIXTURE_PASS` and stays on a
green screen. A failure panics instead of waiting for input. The same binary
can be built for the browser. Without `gltf`, it prints a feature reminder
and exits.

## `gltf_pose_check`

Requires `--features gltf`. It runs the same generated GLB pixel assertions as
`tests/gltf_pose.rs`. A mesh imported under a non-origin rest transform moves
to a spawn's position and yaw through a replacement node pose. The checks
cover CPU and retained draws, reflected rest/replacement/parent transforms,
inverse-transpose lighting, posed bounds, and repeated retained draws without
new geometry uploads. A second generated scene edits a parented blind and prop
through independent hierarchy instances. It checks child rotation under
nonuniform parent scale, exact rest reset, bounds-based culling and globally
sorted transparent descendants after parent depth changes. A passing run prints
`GLTF_HIERARCHY_FIXTURE_PASS` and then
`GLTF_POSE_FIXTURE_PASS` and stays green. Without `gltf`, it prints a feature
reminder and exits.

## `gltf_morph_check`

Requires `--features gltf`. It runs the generated GLB assertions shared with
`tests/gltf_morph_draw.rs`. Two independently weighted instances reuse shared
rest data and update their own retained geometry. The fixture checks authored
defaults, split triangle chunks, evaluated and generated normals, reflection,
deformed bounds/culling and transparency sorting, pose compatibility, repeated
unchanged weights without uploads, and old queued geometry surviving replacement
or drop. A passing native/WebGL run prints `GLTF_MORPH_FIXTURE_PASS` and stays
green. Without `gltf`, it prints a feature reminder and exits. Shape keys are
evaluated on the CPU; shader morphing, tangent morphs, clips and skinning are
outside this example.

## `gltf_animation_check`

Requires `--features gltf`. A generated self-contained GLB drives a child
transform and morph weight together, using caller-supplied clip time. The
native/WebGL fixture exercises interpolation and playback controls alongside
the existing hierarchy/morph drawing path. A passing run prints
`GLTF_ANIMATION_FIXTURE_PASS` and stays green. No game or Blender assets are
required. CUBICSPLINE, clip blending and skinning are outside this example.

## `material_depth_check`

An OpenGL/WebGL shader regression that exits after checking rendered pixels.
It verifies that opaque surfaces hide glass, transparent surfaces keep depth
testing without writing depth, subsequent clears reset depth, and changing a
custom texture preserves the bindings of earlier queued draws. It also
exercises adaptive streaming-buffer growth, reuse with smaller geometry, and
recreation after the configured capacity changes. A failure panics instead of
waiting for input.

## `cursor_capture`

`G` calls `set_cursor_grab(true)` and `Tab` calls `set_cursor_grab(false)`.
The on-screen line is `cursor_capture`, and the yellow lines are
`cursor_capture_events` for this frame. Desktop confirms `Active` in the same
call. A browser may stay `Denied` until the next click; that click asks once,
inside the pointer event, while the earlier request is still wanted. Escape is
not handled by the example, so the browser's unlock shows as `Lost` until the
next click. `Q` quits. The frame loop does not call `set_cursor_grab`.

## `video_texture`

Requires `--features video`. The first argument is a clip path (default:
`clip.mp4` in the working directory). Space plays/pauses, M toggles mute, and R
stops and returns to the beginning; press Space again to play. The sample
updates the decoder each frame and draws its `Texture2D` with aspect ratio
preserved. That texture can also be assigned to a 3D mesh. The clip itself is
not bundled. Without `video`, the example prints a feature reminder and exits.

Native playback requires FFmpeg and FFprobe; native soundtrack support is
currently Windows. Browser hosting requires `mq_video.js` after the macroquad
bundle, browser-supported codecs, and an explicit user interaction for audible
playback. See [video setup and limitations](../docs/video.md) before running or
hosting the sample.

## `canvas_host`

A self-contained browser host fixture with optional density, framebuffer pixel/
edge budgets and a 30 FPS cap, using the supported host API. The Rust example
draws a mouse/touch target and measured text bounds and reports current logical/
physical coordinates. Build for `wasm32-unknown-unknown`, copy `canvas_host.wasm`
beside `canvas_host.html`, and serve the repository root over HTTP. Inspect
`window.macroquadHost.getReport()` and `window.canvasHostFixture`; no sizing or
scheduler overrides are needed. The native example uses ordinary `Conf`.
See [browser host configuration](../docs/browser-host.md) for runtime settings,
requested/effective reporting, rounding and pacing limitations.

## Assets

| Path | Role |
| --- | --- |
| `assets/triangle.gltf`, `assets/triangle.glb` | Hierarchy, box collider, spawn, and a 1×1 base-color image used by the loader tests. Regenerate with `assets/gen_triangle_gltf.py`. |
| `assets/textured_quad.glb` | Checkerboard drawn by `gltf_textured`, `gltf_retained` and `gltf_pollable`. Regenerate with `assets/gen_textured_quad.py`. |

Both generators use Python and do not require Blender. Historical game-round
prototypes, including Marble Maze and Cinder Court, and their level assets are
not checked in. The `room.glb` in the Blender tutorial is an export you create.

The guides are [`docs/3d-guide.md`](../docs/3d-guide.md), [`docs/blender-to-macroquad.md`](../docs/blender-to-macroquad.md), [`docs/ui-2d-guide.md`](../docs/ui-2d-guide.md), and [`docs/browser-host.md`](../docs/browser-host.md). The 3D guide also covers pointer capture, queued font-atlas lifetime, and the logical coordinates `Camera3D::screen_to_ray` expects.
