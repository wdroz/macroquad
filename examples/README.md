# Examples added on this fork

The other files in this directory are the upstream macroquad samples. This
page lists this fork's collision, camera, glTF, retained geometry, material
regression, and video examples. The `gltf` feature enables the glTF loaders;
`video` independently enables video playback.

| Area | Examples | What to inspect |
| --- | --- | --- |
| Shapes and rays | `aabb_overlap`, `sphere_overlap`, `ray_cast`, `plane_hit`, `triangle_hit`, `obb_overlap`, `capsule_overlap` | Inclusive overlaps, hit normals, shape sizes, nearest hits |
| Motion queries | `sweep_hit`, `sweep_obb`, `slide_solids` | Continuous box/sphere hits, oriented cover, slide against all four solid shapes |
| Camera and debugging | `picking_ray`, `frustum_cull`, `debug_draw` | Screen picking, conservative frustum queries, shape wireframes |
| Imported levels | `gltf_textured`, `gltf_retained` | Immediate GLB draw, shared retained geometry and transformed culling |
| Renderer behavior | `retained_mesh`, `material_depth_check` | Immutable buffers and draw order, independent depth writes/testing, queued samplers |
| Video | `video_texture` (`video` feature) | Play/pause, synchronized soundtrack, and a reusable frame texture |

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
cargo run --example picking_ray
cargo run --example frustum_cull
cargo run --example debug_draw
cargo run --features gltf --example gltf_textured
cargo run --example material_depth_check
cargo run --example retained_mesh
cargo run --features gltf --example gltf_retained
cargo run --features video --example video_texture -- path/to/clip.mp4
```

These commands run from the fork root and open a window. To compile a sample
without opening a window, use `cargo build --example <name>` with the same
feature flags and omit runtime arguments after `--`. Pure collision and scene-loader tests can run without graphics;
rendering and texture uploads need a graphics context.

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
support frustum culling, and the display reports submitted chunks. The
adapter preserves current-material drawing; custom lighting and front-face
selection use `GltfModelTransform`. The default material is unlit and does not
cull, so it also draws the reflected instance. See the retained geometry section of the
[3D guide](../docs/3d-guide.md) for that shader contract.

This example also builds without `gltf`, in which case it prints a feature
reminder and exits. Escape quits the feature-enabled sample.

## `material_depth_check`

An OpenGL/WebGL shader regression that exits after checking rendered pixels.
It verifies that opaque surfaces hide glass, transparent surfaces keep depth
testing without writing depth, subsequent clears reset depth, and changing a
custom texture preserves the bindings of earlier queued draws. It also
exercises adaptive streaming-buffer growth, reuse with smaller geometry, and
recreation after the configured capacity changes. A failure panics instead of
waiting for input.

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

## Assets

| Path | Role |
| --- | --- |
| `assets/triangle.gltf`, `assets/triangle.glb` | Hierarchy, box collider, spawn, and a 1×1 base-color image used by the loader tests. Regenerate with `assets/gen_triangle_gltf.py`. |
| `assets/textured_quad.glb` | Checkerboard drawn by `gltf_textured` and `gltf_retained`. Regenerate with `assets/gen_textured_quad.py`. |

Both generators use Python and do not require Blender. Historical game-round
prototypes, including Marble Maze and Cinder Court, and their level assets are
not checked in. The `room.glb` in the Blender tutorial is an export you create.

The guides are [`docs/3d-guide.md`](../docs/3d-guide.md) and [`docs/blender-to-macroquad.md`](../docs/blender-to-macroquad.md).
