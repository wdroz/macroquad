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

Empties are the usual markers (Add → Empty → Plain Axes). The loader reads each node in two ways. Recognized node extras win. A name suffix is the fallback when `mq_role` is absent, unrecognized, or not a string.

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

The suffix compare is case-sensitive and accepts `_collider`, `_trigger`, or `_spawn`, followed optionally by one dot and one or more ASCII digits. `Crate_collider`, `Crate_collider.001`, and `patrol_spawn.001` match. `Player_Spawn`, `patrol_spawn.001.002`, `patrol_spawn.`, and `patrol_spawn.001x` do not. A matching empty still needs `mq_shape`, or a mesh, before it is emitted as a collider. Extras override a suffix: a node named `pad_collider.001` with `mq_role` set to `spawn` is a spawn, not a collider. Imported names retain the duplicate tail.

Add a string custom property `mq_tag` to select a marker independently of its
name: `scene.marker_tag(spawn.node) == Some("player")`. Tags are stored in
`scene.marker_tags`, indexed by the same scene-local node indices as `scene.nodes`.
Every loaded node has an entry, even without a marker role; a tag alone does
not create a spawn or collider. Missing, nested, or non-string tags return
`None`. Strings with an unpaired UTF-16 JSON escape also return `None`; an
undecodable unrelated extras key or value does not hide later marker fields.
Empty strings, case, whitespace, and Unicode are preserved. Nodes
outside the loaded scene have no entries. Cloning or changing the up axis keeps
the tags unchanged. `GltfSpawn` still has `node`, `name`, and
`position`; use `scene.marker_rotation(spawn.node)` for its world rotation, or
`scene.nodes[spawn.node].world` for the full matrix. The rotation includes its
parents and any up-axis conversion applied to the scene. Choose the player's
collision radius in the game.

If you construct a complete `GltfScene` literal, include `marker_tags` with
one `None` per node (or the tags you supply). This field was added with marker
tag support; older complete scene literals need that migration. `GltfNode`,
`GltfCollider`, and `GltfSpawn` literals are unchanged. Normal loader calls
populate the metadata automatically. Keep `marker_tags` aligned when editing
or reordering `nodes` yourself.

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
   `mq_shape = sphere`, and `mq_tag = goal`. The resulting trigger has radius `0.6`.
3. Add a Plain Axes empty at `(0, 0, 0.4)`, name it `player_spawn`, and add
   `mq_role = spawn` and `mq_tag = player`. This tutorial interprets its position as the center of
   a sphere of radius `0.4`, resting on the floor. The file stores no body size.
4. Give the cubes simple opaque base-color materials. Empties have no draw
   geometry; the game below outlines the goal sphere itself.

## 2. Export from Blender

File → Export → glTF 2.0 (`.glb` / `.gltf`).

1. **Format: glTF Binary (`.glb`)** when the game will draw the level. `GltfModel` only accepts a `.glb`. `GltfScene::from_bytes` also accepts a `.gltf` whose buffers are `data:` URIs. External `.bin` files are unsupported; an external image is an error when a loaded primitive samples it.
2. **Include → Custom Properties** (`export_extras=True` in Python), so `mq_shape`, `mq_role`, and `mq_tag` reach the node extras. A name suffix alone can still assign a role if this box is unchecked, but it cannot supply the shape of an empty collider/trigger or its tag; the tutorial needs this option enabled.
3. **Transform → +Y Up** stays checked.
4. **Data → Mesh**: apply modifiers if the collider should see the modified surface, and export UVs, normals, and tangents when the mesh will be drawn or shaded. Leave the exporter's **Shape Keys** option enabled when the object has them (it sits under Mesh or Animation, depending on the Blender version). `GltfMorphData` reads the POSITION and NORMAL deltas. `GltfMorphModel` draws independently weighted instances of those shape keys. Tangent morphs are not evaluated.
5. **Data → Material**: export materials, and keep images packed into the `.glb` (the exporter's automatic image mode). PNG, JPEG, and WebP are decoded. A WebP image uses `EXT_texture_webp`. A normal map and a metallic-roughness map are loaded with the draw model; occlusion is not.
6. Leave Draco compression off. This loader does not decode it. Enable animation export for node TRS and shape-key weight clips. The optional `GltfAnimationData` sidecar supports LINEAR and STEP; CUBICSPLINE returns an explicit error, so bake unsupported curves to LINEAR keys when needed. Skinning remains unsupported.
7. Do not export a camera or a light unless you want those nodes in the hierarchy. They are not special-cased.

Custom property names are exactly `mq_shape` and `mq_role`, string values `box` / `sphere` / `capsule` and `collider` / `trigger` / `spawn`. The optional `mq_tag` property is any string chosen by the game, such as `player` or `guard`.

Export the file to `assets/room.glb` in your game project. Ensure the marker
empties are included if exporting only selected objects. Keep the `.blend` as
the editable source. No Blender installation is required when running the game.

## 3. Inspect the CPU scene

Parsing bytes does not need a window. For a file embedded at compile time, use
`include_bytes!("../assets/room.glb")` from `src/main.rs`. The following snippet
accepts the file's bytes so it also works with another loading mechanism.

```rust
use macroquad::gltf::{GltfColliderShape, GltfScene};
use macroquad::math::Sphere;
# fn inspect(bytes: &[u8]) {
let scene = GltfScene::from_bytes(bytes).expect("level");

// Check the authoring contract before starting gameplay. These counts describe
// this six-object tutorial scene; change them when authoring a different level.
assert_eq!(scene.solid_colliders().count(), 4);
assert_eq!(scene.trigger_colliders().count(), 1);
assert_eq!(scene.spawns.len(), 1);
assert_eq!(scene.marker_tags.len(), scene.nodes.len());
let goal = scene.trigger_colliders()
    .find(|c| scene.marker_tag(c.node) == Some("goal"))
    .expect("goal trigger: export its mq_shape and mq_tag");
let GltfColliderShape::Sphere(goal_shape) = goal.shape else {
    panic!("goal needs mq_shape = sphere");
};
assert!((goal_shape.radius - 0.6).abs() < 1e-5);

let spawn = scene
    .spawns
    .iter()
    .find(|spawn| scene.marker_tag(spawn.node) == Some("player"))
    .expect("spawn");
let body = Sphere::new(spawn.position, 0.4);

for collider in scene.solid_colliders() {
    if collider.shape.overlaps_sphere(&body) {
        // `body` touches or overlaps this solid; boundaries are inclusive.
    }
}
# }
```

CPU parsing needs no graphics context and is suitable for tests. The floor is
expected to overlap the resting player's sphere because touching counts.
Checking only `scene.nodes` would miss an unrecognized marker: the empty can
be present as a node while absent from `scene.colliders`. Assert the emitted
role, shape and tag used by the game. Tags survive Blender's `.001` name tail,
but they are not unique identifiers: duplicating a tagged object duplicates its
tag too. When the game requires one player spawn, check that exactly one spawn
has its tag rather than silently accepting the first of several.

To preserve the facing direction of a spawn, rotate its empty in Blender before
exporting. Reading only `spawn.position` loses that orientation. After loading
and any up-axis conversion, obtain the rotation through its node:

```rust
use macroquad::gltf::GltfScene;
use macroquad::math::{Quat, Vec3};
# fn player_pose(scene: &GltfScene) -> Option<(Vec3, Quat)> {
let spawn = scene.spawns.iter()
    .find(|spawn| scene.marker_tag(spawn.node) == Some("player"))?;
let rotation = scene.marker_rotation(spawn.node)?;
// Choose the local forward axis used by the player's mesh or controller.
let forward = rotation * -Vec3::Z;
# let _ = forward;
# Some((spawn.position, rotation)) }
```

`marker_rotation` returns a unit quaternion from the stored node world matrix,
including parent rotation. It returns `None` for an invalid index or when no
finite rotation can be obtained. The helper reads the CPU scene; editing an
independent hierarchy pose does not change it. Keep the full node matrix when
the application also needs scale or shear.

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
        .find(|spawn| scene.marker_tag(spawn.node) == Some("player"))
        .map(|spawn| spawn.position)
        .expect("player_spawn");
    let (mut aabbs, mut obbs, mut spheres, mut capsules) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for collider in scene.solid_colliders() {
        match collider.shape {
            GltfColliderShape::Aabb(shape) => aabbs.push(shape),
            GltfColliderShape::Obb(shape) => obbs.push(shape),
            GltfColliderShape::Sphere(shape) => spheres.push(shape),
            GltfColliderShape::Capsule(shape) => capsules.push(shape),
        }
    }
    // Markers are all we need after extracting the solids and spawn. Drop the
    // separate CPU scene's mesh/image data as well as the input file bytes.
    let triggers: Vec<_> = scene.trigger_colliders().cloned().collect();
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

For an axis-aligned player body, `collider.shape.sweep_aabb(&body, delta)`
dispatches to the existing body sweep for every imported shape without
allocating or converting it. The receiver is the static obstacle; `body` and
`delta` describe the incoming motion, all in world space. The caller selects
the smallest time of impact; using a center ray or turning an authored
sphere/capsule into a box changes which obstacles it hits. This caller-owned
loop queries one translation and excludes triggers:

```rust
use macroquad::prelude::*;

fn first_solid_hit(scene: &GltfScene, body: Aabb, delta: Vec3) -> Option<SweepHit> {
    let mut first: Option<SweepHit> = None;
    for collider in scene.solid_colliders() {
        let Some(hit) = collider.shape.sweep_aabb(&body, delta) else {
            continue;
        };
        if let Some(previous) = first {
            if hit.t >= previous.t {
                continue; // Equal times keep the first collider in stored order.
            }
        }
        first = Some(hit);
    }
    first
}
```

`hit.t` is a fraction of `delta`, and `hit.normal` points out of the static
solid toward the moving body. A resting touch permits sideways movement; an
initial penetration can report time zero and needs a separate contact/push-out
policy. The dispatch preserves each primitive query's exact hit and invalid-input
behavior. It does not select candidates, pick the nearest hit or provide
sliding, gravity, grounding or stair stepping. Trigger roles belong to
`GltfCollider`, so use `solid_colliders()` for blocking queries and select
triggers explicitly when those are wanted. The moving body stays axis-aligned
throughout the query, including non-Y-up scenes. A moving gate can supply its
current `hierarchy.collider_shape(&scene, collider)` and call `sweep_aabb` on
that shape in place of the stored `collider.shape`; that is still one query
against a static snapshot, not a moving-platform solver.

For repeated kinematic movement, `CharacterController::move_body` combines the
body sweeps with sliding, ground snapping and a slope limit. Build
`CharacterSolids { aabbs: &aabbs, obbs: &obbs, spheres: &spheres,
capsules: &capsules }` from the four **trigger-filtered** arrays above. The
controller cannot identify trigger roles from those shape slices; keep trigger
overlap queries separate. Pass a world-space `Aabb`, caller-owned velocity and
elapsed time, then retain the returned `CharacterMotion::body` and `velocity`.
Its `on_ground` and optional `ground_normal` describe support at the final pose.

Choose `up` to match the loaded scene, set `max_slope_angle` in radians and
`ground_snap_distance` in world units. The body remains aligned to the world
axes regardless of `up`. Apply gravity in the game; positive input velocity
along `up` skips grounding and snapping for takeoff. For authored stairs, call
`move_body_with_step(body, velocity, dt, solids, step_height)` and retain its
`motion`. The optional height is in world units; the full body is swept up,
forward and onto a walkable landing. Its `step` diagnostics explain clearance,
landing and slope failures. Keep the tick short enough for the tread spacing;
one call does not solve an arbitrary flight of stairs. Capsule players and
moving platforms remain separate work. See the
[3D guide](3d-guide.md) and `CharacterController` rustdoc for the movement
contract and a complete example.

For a sphere or a fixed multi-sphere hull, reuse those same four solid-only
arrays with `slide_sphere_solids` or `slide_spheres_solids`. They already accept
static capsule markers and return a center or one shared translation,
respectively. If the game needs the time of one impact instead, primitive
`Sphere` sweeps cover AABBs, OBBs, spheres and capsules. A sphere player can use
`Sphere::sweep_capsule` directly for one static capsule marker, with the hit
normal and point in world space. Apply `delta * hit.t` to its center; the hit
point belongs to the obstacle. See the current [sweep table](3d-guide.md#sweep)
for initial-contact rules and numerical limits. Neither path sweeps a rotating
body.

Use `slide_spheres_solids_with_contacts` when a hull needs the selected solid,
sample and outward normal for scrape/stop feedback. Its `solid_index` belongs
to one of the four extracted arrays, so retain a parallel mapping to the
original collider/node if gameplay needs marker tags or identity. It does not
index `scene.colliders`. Events can repeat and include a blocked final probe
that the hull did not reach; use them as [solver diagnostics](3d-guide.md#rigid-groups-of-spheres),
and test triggers separately at the moved pose.

For a larger set of authored colliders, the optional use of `UniformGrid`
can narrow the candidates before the shape queries above. Store a conservative
world AABB under each collider's caller-chosen `u32` index, retain the mapping
to `scene.colliders` yourself, and query by AABB or ray. Returned indices are
unique and numerically sorted; they do not imply nearest-hit order or solid
roles. Keep triggers separate and still test the original collider shape.
Update the bounds after pose changes. Enclosing the start and end bounds is
enough for a straight sweep; sliding can deflect beyond that box. Use an envelope
that also covers deflection, and handle initial overlap against the complete
solid set before narrowing movement candidates. Steps and snapping need their
additional paths covered as well. See
[Uniform grid candidates](3d-guide.md#uniform-grid-candidates) for a complete
example, [sliding bounds](3d-guide.md#candidate-bounds-for-sliding),
replacement/rebuild semantics and invalid-input behavior. The loader
and character controller do not create or own a grid automatically.

For mass-based objects, `RigidBody` offers a separate integration and discrete
pair-response path. Its AABB/sphere geometry is local to `body.position`;
imported `GltfColliderShape` geometry is already in world space. Keep the imported
shape at a zero body position, or explicitly convert it to local coordinates,
so its translation is not applied twice. A sphere body can use
`resolve_static_obb` directly for an imported world-space OBB. Capsule body
response is not provided; keep capsule solids on the supported query/slide
paths. Filter out triggers before any response and choose the application
time step and pair order. The [physics guide](3d-guide.md#physics) explains
static versus kinematic bodies, restitution and transactional errors.
The maintained [`rigid_body_joint` example](../examples/README.md#rigid_body_joint)
adds a bounded fixed-step clock and one caller-ordered distance joint. Its
residual/overlap probe shows why moving the joint correction after contacts
can reintroduce penetration. Use that recipe when adding a constrained prop;
the imported level loop above remains kinematic.

## Add a mission log or pause menu

The level loop above switches to `set_default_camera()` before drawing its
HUD. Keep that boundary when adding a menu. If the world uses a custom
material, finish its pass and call `gl_use_default_material()` first. The
[combined world/HUD recipe](3d-guide.md#draw-a-hud-over-a-level) shows a clipped
mission log with ordinary text and shapes; it needs no raw renderer access.

Use logical window pixels for the HUD and `mouse_position_logical()` for its
custom hitboxes. Select the camera before a synchronous `with_clip_rect`
scope, and draw unclipped headings or controls after the scope. Built-in UI
groups provide their own scrolling and clipping; a custom drawing scope does
not enclose their deferred commands.

For a pause/settings panel, use `LayoutSettings` to reserve a scrollable
content group and a reachable action row, then give each keyboard-operable
`Button` a stable `.id(hash!("action-name"))`. The
[UI and 2D guide](ui-2d-guide.md#make-buttons-usable-from-the-keyboard) shows
Tab traversal and a confirmation modal that restores the opener's focus.
Keep the pause flag in the game: skip player movement, world shortcuts and
animation advancement while paused, but continue drawing the level and UI.
Borrow `root_ui()` once per UI block and release it before awaiting the next
frame. These UI additions do not require Blender exports or new level markers.

For settings, opt existing `Checkbox` and `Slider` builders into
`keyboard_focus(true)` using stable, unique IDs. Sliders also require a finite
positive `keyboard_step` in application value units; their numeric field and
track are two consecutive Tab stops. Disable unavailable settings with
`enabled(false)`. The [checkbox](ui-2d-guide.md#make-checkboxes-usable-from-the-keyboard)
and [slider](ui-2d-guide.md#make-sliders-usable-from-the-keyboard) recipes describe
key edges, endpoints and invalid input. Run `keyboard_sliders_check` to inspect
the mixed menu before integrating it into your level loop.

Use `Label::wrap(ui.available_width())` for descriptions that should expand the
scrollable settings content after resize. `Label::measure` uses the active
skin and the same width for a caller-owned layout; the
[wrapped-label recipe](ui-2d-guide.md#wrap-and-draw-text-explicitly) covers
measurement, margins and spacing. `draw_rectangle_rounded_lines` can outline
a custom panel with a full inward stroke; the
[rounded-panel recipe](ui-2d-guide.md#draw-a-rounded-panel) explains its radius
and thickness limits. Root UI labels use logical window units; immediate
outlines use the current drawing camera, so select the default camera for a HUD.

## Move one imported object

`model.draw` and `draw_culled` submit every chunk with the current material.
To hide a collected prop or lift a gate without cloning vertices, keep that
one retained model and submit the chunks you want:

```rust,no_run
use macroquad::prelude::*;
# fn draw_level<'a>(
#     renderer: &mut GltfMaterialRenderer,
#     model: &'a RetainedGltfModel,
#     camera: &Camera3D,
#     gate_lift: f32,
# ) -> Result<(), String> {
set_camera(camera);
let mut frame = renderer.begin(camera.position, camera.target - camera.position)?;
frame.set_lighting(Some(GltfLighting::default()))?;
for part in &model.parts {
    if part.name == "collected_cell" {
        continue;
    }
    let transform = if part.name == "gate" {
        Mat4::from_translation(Vec3::Y * gate_lift)
    } else {
        Mat4::IDENTITY
    };
    frame.submit_retained_part(part, transform)?;
}
let _stats = frame.draw();
# Ok(())
# }
```

`submit_retained_part` borrows the existing chunk and its GPU buffers. The
extra matrix is applied to coordinates that are already baked into the scene,
then the parent model matrix active at `draw` is applied as well. These
additional-transform calls do not read a rest pose. A rotation around a chosen
point is `Mat4::from_translation(pivot) * rotation *
Mat4::from_translation(-pivot)`. Submitting the whole model and one of its parts
draws that part twice. This draw path does not move colliders, triggers, or
spawns. `submit_part` is the same call for a CPU `GltfDrawMesh`. To rotate
around the node's authored origin, use the pose sidecar below instead of
building that pivot by hand.

Construct `GltfMaterialRenderer` once inside the running application and keep
it. Each `begin` starts unlit; `set_lighting(Some(..))` opts that collection
into hemisphere and directional diffuse light. See
[material passes](3d-guide.md#opt-in-gltf-material-passes) for alpha sorting,
color output, and the shapes this light model does not cover.

## Authored poses and shape keys

`GltfModel::from_bytes_with_pose` keeps the authored node world matrices next
to the baked chunks. `GltfModelLoader::take_pose` returns the same sidecar
after a pollable load, together with `take_model` and `take_material_maps`.
`GltfNodePose::set_world` replaces one node's world matrix. The draw transform
is `replacement * inverse(rest)`, so a rotation is around the authored node
origin, including a spawn yaw stored on that node. Setting the world matrix
back to `pose.rest_world()` restores the baked placement.

```rust,no_run
use macroquad::prelude::*;
# fn lift_gate(bytes: &[u8], part: &RetainedGltfDrawMesh) -> Result<(), String> {
let (cpu, rest) = GltfModel::from_bytes_with_pose(bytes)?;
let mut pose = rest.node(part.node)?;
let _model = RetainedGltfModel::from_model(cpu, false).map_err(|e| e.to_string())?;
pose.set_world(pose.rest_world() * Mat4::from_translation(Vec3::Y * 2.0))?;
part.draw_posed(&pose)?;
# Ok(()) }
```

`rest.hierarchy()` builds an independent `GltfHierarchyPose`. Edit one node's
`GltfLocalTransform` with `set_local`; descendants recompute immediately.
`node_pose(handle)` is the snapshot to pass to `draw_posed`, `posed_bounds`,
or `submit_retained_part_posed`. `reset_local` restores one authored matrix
and `reset` restores the whole instance. A matrix that contains shear uses
`set_local_matrix` instead of translation/rotation/scale. Spawns stay where
`GltfScene` stored them. `GltfHierarchyPose::collider_shape` returns a collider
or trigger in the current worlds and leaves the stored shape at rest. Apply the same
`with_up_axis`
to the scene, the draw model, and the pose sidecar before creating editable
poses. The [hierarchy guide](3d-guide.md#editing-an-imported-hierarchy) shows
culling and transparent sorting with those poses. `examples/gltf_pose_check.rs`
is the pixel fixture.

Use the imported collider size for this query. Rebuilding a default Blender
cube with `Obb::from_world_matrix(node.world)` treats scale as full size and
loses the cube's ±1 vertex bounds. Posed spheres use the delta transform's
largest stretch, including shear, for an enclosing radius. Posed capsules
transform the segment and use the largest transverse stretch for their radius.
These remain primitive approximations; the helper does not turn them into
ellipsoids or evaluate shape-key collisions. Singular rest matrices and
unrepresentable transformed shapes return `None`.

Shape keys are separate from that transform. `GltfMorphData::from_bytes` loads
mesh-local POSITION/NORMAL deltas and the node's default weights. It does not
open a window or upload a mesh. Pass `None` to use the file's weights, or one
finite weight per target. Weights are not clamped. Apply the node's world
matrix, including `with_up_axis`, after evaluation. Weight clips use the
optional animation sidecar below; skinning remains unavailable.

```rust
use macroquad::gltf::GltfMorphData;
use macroquad::math::Mat4;
# fn smile(bytes: &[u8], face_node: usize) -> Result<(), macroquad::gltf::GltfError> {
let morphs = GltfMorphData::from_bytes(bytes)?;
let face = morphs.evaluate(face_node, 0, Some(&[1.0]))?;
let _bounds = face.transformed_bounds(Mat4::IDENTITY)?;
# Ok(()) }
```

The [morph guide](3d-guide.md#cpu-morph-data-and-evaluation) covers missing
normal deltas and the weight rules.

To display the shape key, load a `GltfMorphModel` inside the application and
instantiate it. Each instance shares immutable rest arrays and textures;
changing weights replaces that instance's affected GPU chunks. Node-authored
defaults apply initially, and repeated identical weights cause no uploads.

```rust,no_run
use macroquad::prelude::*;
# fn draw_smile(bytes: &[u8], face_node: usize) -> Result<(), String> {
let source = GltfMorphModel::from_bytes(bytes)?;
let mut face = source.instantiate();
face.set_weights(face_node, &[1.0])?;
face.model().draw(Mat4::IDENTITY);
# Ok(()) }
```

Use `face.model()` with the existing retained part, material, pose and culling
APIs: its normals, bounds and centers describe the current weights. Choose
`source.with_up_axis(up)?` before instantiating when the rest of the level uses
another up axis. `source.pose()` gives the matching rest matrices. For a
loading screen, use `GltfModelLoader::new_with_morph` and
`take_morph_model()`. Updates evaluate and upload synchronously, and the rest
arrays remain in CPU memory. Existing queued draws retain their previous
buffers until submission is finished. The
[morph drawing guide](3d-guide.md#drawing-independent-morph-instances) shows
two independent expressions and the material-renderer path. Shader-based
morphing, tangent updates and deformed normal-map/GGX buffers remain future
work; ordinary `GltfModel` drawing continues to use the undeformed geometry.

To play an exported action, parse `GltfAnimationData::from_bytes(&bytes)` and
choose a clip by `name()`. Create a `GltfAnimationPlayer`, supply elapsed time
with `advance` or absolute time with `seek`, and apply each sample to the
matching hierarchy and/or morph instance. Use `set_paused`, `reset` and
`set_looping` to control the player. Reset rewinds to the first clip sample;
hierarchy and morph resets restore authored rest data. Apply the same
`with_up_axis` to the clip and model/pose. The
[clip guide](3d-guide.md#playing-gltf-clips) shows the full draw path and
interpolation rules. `gltf_animation_check` is a tiny fixture independent of
Blender and game assets. Animation import does not require image decoding or
a window; applying changed morph weights does require the render context.
Apply the sample before reading the current chunk bounds or posed markers.
`sample.apply` reports whether morph weights changed; a `false` result can
still include a successful node transform update. Its atomic pose/morph update
does not roll back a caller's already advanced player clock on failure. The
[animated-level update order](3d-guide.md#update-order-for-an-animated-level)
shows the ownership, reset and collision/draw ordering together.

## CPU images and draw materials

`scene.images` holds the base-color images sampled by loaded primitives, decoded to 8-bit RGBA (`GltfImage`: `width`, `height`, and row-major `pixels` with row 0 at the top). `GltfPrimitive::base_color_texture` is an index into that list, or `None` when the material has no base-color image. Images no loaded primitive samples are omitted. Emissive images are uploaded with `GltfModel`, not stored in this list. Normal and metallic-roughness images are the separate `GltfMaterialMaps` sidecar (`GltfMaterialMaps::from_bytes`), still CPU RGBA and still not a `Texture2D`. Occlusion maps are not loaded. The bytes are the file's stored channels (sRGB for a base-color texture; linear for a normal or metallic-roughness texture), and not a `Texture2D`. `with_up_axis` does not move them.

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
`part.surface`. These current-material draw calls do not apply alpha masks,
choose sidedness, light, sort transparency, skin, animate, or refract. Opt into
`GltfMaterialRenderer` for alpha passes and optional basic lighting, or iterate
`model.parts` and select custom materials for application-specific rendering.

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

The draw-call capacities are upper limits, not a GPU reservation for every material group. Each streaming vertex/index buffer starts at the submitted chunk's actual size and grows geometrically only when a later draw needs more space, bounded by the configured limit. Buffers retain their high-water capacity for reuse; changing the limits releases them for lazy recreation. Clearing queued draws also discards their queued CPU geometry. This keeps large permitted chunks affordable in scenes that contain hundreds of small meshes. Final GLB mesh chunks discard unused vector capacity. The loader decodes and uploads all consumed textures before converting primitives, so source mesh arrays remain alive during image decoding. Conversion releases each source mesh after its last node instance.

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

Both image dimensions stay at or below this limit; a 4096×2048 image becomes 512×256. Smaller images stay unchanged, and the aspect ratio, UVs, and sampler settings are preserved. The cap applies to each embedded base-color, emissive, normal, and metallic-roughness image and its mipmaps. Occlusion maps are not uploaded. `None` is the default, preserving the original full-resolution behavior of the existing loading methods. The cap bounds retained GPU image memory, but the decoder still briefly holds the original image in CPU memory. It does not edit or re-encode the source GLB. Supply `mesh_limits` in the same options value when changing mesh chunk sizes as well.

For an explicit source-size guard, wrap those options with
`options.with_image_limits(GltfImageLimits { max_source_pixels: Some(2048 * 2048),
max_decoded_bytes: Some(16 * 1024 * 1024) })` and pass the result to
`GltfModel::from_bytes_with_settings`. Import `GltfImageLimits` from
`macroquad::gltf_draw`. The loader checks metadata for every consumed base-color,
emissive, normal, and metallic-roughness image before decoding pixels or creating textures. Rejections name
the image and material. Each limit is per image; the byte estimate is source
width × height × 4 for RGBA8, not total model or decoder memory. Existing
constructors and defaults stay uncapped. See the [3D guide](3d-guide.md#loading-and-materials)
for a complete example.

An image that fails these limits needs offline preparation before export or
loading. Upload resizing cannot make it pass a source-size check. Decoder
scratch buffers and higher-bit-depth intermediates can exceed the RGBA8
estimate, so these limits do not guarantee a process-memory ceiling.

For a loading screen, use the same settings with `GltfModelLoader::new` and
call `poll` once per frame, then pass the finished CPU model to
`RetainedGltfModelLoader`. Parsing, decoding one image, and converting one
primitive can each occupy a whole frame. After the next level replaces this
one, finish the old frame, drop the old model, and call
`retire_unused_render_resources` once. Ordinary pause does not do that trim.
`GltfResourceReport::collect` estimates the bytes of the models you still
hold; it is not process or driver memory. The
[pollable construction](3d-guide.md#pollable-gltf-construction) and
[retirement](3d-guide.md#retiring-unused-render-resources) sections show the
calls.

The cap does not resize the separate `GltfScene::images` CPU list. Retaining
that scene keeps its decoded images and local mesh arrays alive even after
`RetainedGltfModel::from_model(cpu, false)` releases draw-model geometry.
Use `GltfScene::from_bytes_with_image_limits(bytes, limits)` to apply the same
per-image rejection policy to that CPU loader's sampled base-color images.
Extract the markers you need and drop the scene data when appropriate, as in
the sample. `model.stats` reports counts rather than memory estimates or
process/driver memory measurements.

Node `world` is the glTF hierarchy: local translation, rotation, and scale composed with the parent. Mesh positions stay in mesh-local space on `GltfScene`. `GltfModel` has already multiplied them by `world`.

## Samples in this repository

The committed files below make builds and tests independent of Blender. The room is exported by headless Blender; the triangle and checkerboard generators use ordinary Python.

| File | What it is |
| --- | --- |
| `examples/assets/triangle.gltf` and `triangle.glb` | A small hierarchy, one box collider (`crate_collider`, extras), one spawn (`player_spawn`, name suffix), and a 1×1 base-color image (`pixel`, RGBA 10, 20, 30, 255). |
| `examples/assets/textured_quad.glb` | A 4×4 checkerboard quad used by `examples/gltf_textured.rs`. |
| `examples/assets/room.glb` | Original 10-node room: four visible cubes, four box-collider empties, one sphere trigger and one spawn. No textures or external files. |
| `examples/assets/gen_room_blender.py` | Regenerates the room with Blender 4.5+; resets a separate headless scene and exports explicit roles, empty shapes and tags with `export_extras=True`. |
| `examples/assets/gen_triangle_gltf.py` | Regenerates the triangle pair. Does not need Blender. |
| `examples/assets/gen_textured_quad.py` | Regenerates the checkerboard. Does not need Blender. |

```sh
cargo run --features gltf --example gltf_room
cargo test --features gltf --test gltf_room
cargo run --features gltf --example gltf_textured
cargo run --features gltf --example gltf_textured -- path/to/room.glb
cargo run --features gltf --example gltf_retained
cargo run --features gltf --example gltf_pollable
cargo run --features gltf --example gltf_materials -- path/to/room.glb
cargo run --features gltf --example gltf_material_check
cargo run --features gltf --example gltf_shading
cargo run --features gltf --example gltf_pose_check
cargo run --features gltf --example gltf_morph_check
cargo run --features gltf --example gltf_animation_check
```

With no path, `gltf_textured` shows the checkerboard. A path is loaded with
`load_file`. `gltf_retained` uses that fixture for three moving, shared-buffer
instances, including a reflection. The [example index](../examples/README.md)
also covers picking, frustum culling, contacts, sweeps, and slide queries.

For exported alpha cutouts or blended surfaces, `gltf_materials` opts into
`GltfMaterialRenderer` and prints unsupported-feature warnings with node and
material identifiers. This base/emissive preview applies alpha, sidedness and
color conversion; the example also enables optional hemisphere/directional
lighting. Press L to compare the lit view with the preview. Its second instance
uses a reflected, nonuniform scale. The renderer transforms imported normals
with the inverse transpose and keeps textured emission independent of lighting;
`KHR_materials_unlit` always displays base color only.

Use `frame.set_lighting(Some(GltfLighting::default()))?` for a starting light,
then set world-space `hemisphere_up`, `directional_direction` (toward the light),
and linear RGB `sky_color`, `ground_color`, and `directional_color` to suit the
scene. `submit` stays diffuse and ignores metallic, roughness and maps.
`submit_shaded` samples the retained normal and metallic-roughness images:
linear texels, distinct UV sets, `KHR_texture_transform`, and tangents,
including a mirrored node's handedness. It adds directional GGX highlights on
top of the hemisphere. Occlusion, image-based lighting, refraction and shadows
are not implemented. Load the maps with
`GltfModel::from_bytes_with_material_maps` (or `GltfModelLoader::take_material_maps`).
`from_bytes` validates them and drops the handles. The renderer does not alter existing current-material draws. See the
[material passes guide](3d-guide.md#opt-in-gltf-material-passes) before replacing
a custom renderer, especially its transparency ordering and output-space limits.

## What the loader does not do yet

- `GltfScene::with_up_axis` does not change a `GltfModel` or a `GltfModelPose`, and those calls do not change the scene. Call all three with the same axis when the draw, the pose, and the markers should leave Y-up. A camera can instead keep `up: Vec3::Y` and skip them. That is what the examples do.
- `GltfModel` still uploads its own GPU textures, and only from a `.glb`. Sampler wrap and filter stay on that draw path. `GltfScene` and `GltfMaterialMaps` do not turn their RGBA bytes into a `Texture2D`.
- Skins, CUBICSPLINE interpolation, clip blending, tangent morphs and shader-based morphing are not implemented. Optional LINEAR/STEP node TRS and weight clips use GltfAnimationData and GltfAnimationPlayer. `GltfMorphData` evaluates POSITION/NORMAL shape-key deltas on the CPU; `GltfMorphModel` uploads changed instances and exposes their current retained chunks. It does not construct deformed normal-map/GGX tangent buffers. Ordinary `GltfModel` drawing remains undeformed. `GltfNodePose` replaces one node's world matrix for drawing, and `GltfHierarchyPose` edits local transforms so descendants follow. `collider_shape` returns a collider in those worlds; the stored collider list and the spawn points stay at rest. `submit_part` and `submit_retained_part` still apply an extra matrix to baked coordinates and do not read that rest pose. A whole-model retained transform moves every chunk together and also leaves colliders where the CPU scene stored them. Full PBR (image-based lighting, occlusion, shadows, transmission) is not implemented. Normal and metallic-roughness maps are loaded and retained (distinct UV sets, `KHR_texture_transform`, and tangents, including mirrored handedness) and `submit_shaded` samples them. Material passes and lighting are opt-in; ordinary `draw` calls still use the current material.
- The six-object tutorial above is a reader-authored `assets/room.glb`. The separate maintained `examples/assets/room.glb` has four visual meshes, four box-collider empties, `goal_trigger.001` (sphere, tag `goal`) and `player_spawn.001` (tag `player`). Its shared [loading contract](../examples/support/room_scene.rs) checks exact counts, empty shapes, dimensions and tags before play. Its [headless tests](../tests/gltf_room.rs) also strip roles to exercise duplicate suffix fallback and strip shapes to verify empty colliders disappear. The historical Marble Maze and Cinder Court games are not distributed here.

Regenerate the maintained room from the repository root with:

```sh
blender --background --factory-startup --python examples/assets/gen_room_blender.py
```

The room uses Blender Z-up dimensions in the generator and glTF Y-up in the
example. It has an open front, walls at X = ±4 and Z = -4, a floor top at Y = 0,
a player center at `(0, 0.4, 2)` and a goal at `(0, 0.4, -2)` with radius 0.6.
WASD slides a radius-0.4 sphere at fixed height, R resets and Escape exits.
All four solids are queried together; triggers never enter the solid list.
This example adds no gravity, jumping or mesh collision. Regeneration may
change exporter metadata/vertex ordering between Blender versions; the loaded
contract and geometry tests, rather than binary equality, define the fixture.
