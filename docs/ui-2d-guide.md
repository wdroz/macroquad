# UI and 2D guide

This guide describes the APIs available today. The unchecked UI and 2D items in
the [fork roadmap](../ROADMAP_3D.md#priorities) are proposals, not additional
helpers you can call yet. Start with the existing immediate-mode UI, `Camera2D`
and texture drawing; `macroquad-tiled` and `macroquad-platformer` already provide
companion support for tile maps and 2D platformer collisions.

## Choose a coordinate space

| Space | Units and origin | APIs |
| --- | --- | --- |
| Window/UI | Logical window units, top-left origin, X right and Y down | `screen_width`, `screen_height`, `mouse_position_logical`, `touches_logical`, `root_ui` |
| Framebuffer | Physical pixels, top-left origin for the position helpers | `mouse_position_physical`, `touches_physical`, `logical_to_physical`, `physical_to_logical` |
| Camera world | Application-defined world units transformed by the active camera | `Camera2D`, `set_camera`, window-space `screen_to_world`/`world_to_screen` |
| Sprite source | Pixels in the original texture | `DrawTextureParams::source` |

Use logical input for HUD hitboxes and ordinary window-space picking. Do not
multiply it by DPI before passing it to `Camera2D::screen_to_world`.
`screen_dpi_scale()` is the current ratio between logical units and physical
framebuffer pixels. Recompute size-dependent layouts when the window resizes;
the density conversions use the current ratio. See
[`input_coordinates`](../examples/README.md#input_coordinates) for mouse,
touch and density diagnostics, and the [browser host guide](browser-host.md)
for canvas budgets and pacing.

## Draw a world, then a HUD

`Camera2D::from_display_rect` maps a world rectangle onto the window. On the
window, a positive rectangle height gives an upward-positive world Y axis;
use a negative height and matching Y origin when you want downward-positive Y
(as in `examples/platformer.rs`).
`target`, `offset`, `zoom` and degree-valued `rotation` customize that transform.
Return to `set_default_camera()` for immediate screen-space drawing. `root_ui()`
is rendered at the end of the frame using the default screen camera.

This complete example needs no assets. Copy it into a macroquad application's
`src/main.rs`; clicking the button toggles the marker color.

```rust,no_run
use macroquad::prelude::*;
use macroquad::ui::root_ui;

#[macroquad::main("World and HUD")]
async fn main() {
    let mut selected = false;
    loop {
        clear_background(LIGHTGRAY);
        let camera = Camera2D::from_display_rect(Rect::new(0.0, 0.0, 320.0, 180.0));
        let pointer = camera.screen_to_world(mouse_position_logical());
        set_camera(&camera);
        draw_rectangle(40.0, 50.0, 240.0, 90.0, DARKGREEN);
        draw_circle(pointer.x, pointer.y, 4.0, if selected { RED } else { YELLOW });

        set_default_camera();
        if root_ui().button(vec2(16.0, 16.0), "Toggle marker") {
            selected = !selected;
        }
        next_frame().await;
    }
}
```

The legacy `Camera2D::world_to_screen` and `screen_to_world` methods use window
dimensions even when `render_target` is set. Their viewport mapping also uses
window height to convert the viewport origin. They are not target-pixel
conversion helpers. A camera viewport does not automatically change zoom or
aspect, and the rendering matrix changes Y orientation for render targets.
For an offscreen target displayed in a letterboxed window rectangle, the
application must map input through that display rectangle and the target's
dimensions/orientation before unprojecting. A supported target-aware helper
and resize/picking fixture remain queued work. The example above uses the
whole window and no render target or viewport.

## Build menus with the existing UI

Submit widgets every frame; store the game/settings values outside the widgets.
Keep IDs stable and unique for stateful windows, groups and editboxes, using
`macroquad::ui::hash` (or your own `u64` IDs). Repeated generated widgets need an
item-specific hash input. Buttons currently return pointer activation; a stable
button focus ID, Tab traversal and Enter/Space activation remain queued.

The existing building blocks include:

- `widgets::Window`, `Group` and `Popup`, with windows/groups providing content
  clipping and scrolling. Use the closure-based `.ui(...)` methods to pair
  begin/end calls.
- `Layout::Vertical`, `Horizontal` and `Free`, explicit positions/sizes, and
  `Ui::same_line` retain their legacy cursor behavior. Opt in to explicit child
  flow with `Ui::set_layout` or `Group::content_layout` below. `Group::layout`
  still controls placement of the group itself in its parent.
- Labels, buttons, editboxes, checkboxes, sliders, progress bars, comboboxes,
  tree nodes and draggable groups. See [`examples/ui.rs`](../examples/ui.rs).
- `Skin` and `StyleBuilder` for fonts, colors, images, margins and nine-slice
  backgrounds. Construct styles once, then pair `push_skin`/`pop_skin` around
  the affected widgets. See [`examples/ui_skins.rs`](../examples/ui_skins.rs).
- `Ui::canvas()` for custom rectangles, lines and images within a UI window.
  `request_space` reserves layout area and returns its position.

Size menus against current logical `screen_width()`/`screen_height()` and leave
room for controls and scrollbars. Set `.movable(false)` on a window whose
position must follow your layout each frame; movable windows remember their
user-controlled position after appearing. Built-in groups/windows clip their
own UI commands. Use `with_clip_rect` below for ordinary `draw_text` and
`draw_texture_ex` calls. Test compact menus
at 390 x 400 as well as desktop sizes; the existing showcase is not a guarantee
of responsive application layout.

## Size rows, columns and scrolling settings

`LayoutSettings` adds explicit padding, per-axis gaps and cross-axis
`LayoutAlign::{Start, Center, End}` to the existing immediate UI. Call
`Ui::set_layout(settings)` once before children in a window/group each frame,
or use `Group::content_layout(settings)`. Settings replace the legacy cursor
margin; widget skin margins and backgrounds remain unchanged. The existing
widget calls still work. `Group::layout` continues to place the group in its
parent independently of its child flow.

```rust,no_run
use macroquad::prelude::*;
use macroquad::ui::{hash, root_ui, widgets, Layout, LayoutSettings};

let mut ui = root_ui();
let size = vec2((screen_width() - 24.).max(1.), (screen_height() - 24.).max(1.));
widgets::Window::new(hash!("settings"), vec2(12., 12.), size)
    .movable(false).titlebar(false).ui(&mut ui, |ui| {
        ui.set_layout(LayoutSettings::new(Layout::Vertical)
            .padding(RectOffset::new(12., 12., 10., 10.))
            .gap(vec2(0., 8.)));
        let width = ui.available_width();
        widgets::Button::new("Resume").size(vec2(width, 36.)).ui(ui);
        widgets::Group::new(hash!("options"), ui.available_size())
            .layout(Layout::Vertical)
            .content_layout(LayoutSettings::new(Layout::Vertical)
                .padding(RectOffset::new(8., 8., 8., 16.))
                .gap(vec2(0., 6.)))
            .ui(ui, |ui| {
                for option in 0..12 {
                    widgets::Button::new(format!("Option {}", option + 1))
                        .size(vec2(ui.available_width(), 32.)).ui(ui);
                }
            });
    });
```

`Ui::available_size`/`available_width` account for padding, the current flow,
indentation and a visible vertical scrollbar. Recompute widths each frame,
including the width passed to existing `wrap_text`. Split the resulting lines
into label widgets; ordinary single-line labels do not automatically wrap.
Children retain explicit oversized dimensions; vertical groups use their
existing clip and scrollbar, including trailing padding after the final child.
Rows are single-line and do not wrap or gain horizontal scrolling. Select a
column at an application breakpoint when a row no longer fits. Oversized
cross-axis children align at the start so their leading edge stays visible.
Use nested groups for mixed flows instead of mixing `same_line` with the
opt-in flow. `Free` widget positions are offsets from the padded origin and do
not advance the flow. Changing a container's size preserves its scroll state
while updating its layout viewport.

For branded/custom drawing, `LayoutCursor::new(rect, settings)` provides the
same pure geometry without a UI or GPU. `allocate(size)` advances the row or
column, `allocate_width(height)` consumes its available width, and
`allocate_at(offset, size)` reserves a free rectangle. `content_size()` includes
trailing padding but excludes a trailing gap. Empty content contains only
padding. Negative/nonfinite padding, gaps and dimensions normalize to zero;
nonfinite origins/offsets normalize to zero, while finite negative offsets
remain valid. Arithmetic saturates to finite values. Caller-selected spaces
remain unchanged: use logical window units for a default-camera HUD, or world
units for custom world drawing. No constraint solver or retained widget tree
is introduced.

Run `cargo run --example responsive_layout` and resize to 390 x 400. The
pause/settings example uses wrapped text, an oversized scroll group and a
pinned action row. `responsive_layout_check` shares the native/WebGL fixture
for resize, geometry, clipping, scroll reachability and pointer input. Clipped
buttons do not react to pointer events outside their group's visible content.

## Draw a rounded panel

`draw_rectangle_rounded(x, y, width, height, radius, color)` draws a filled
panel in the current camera's coordinates. With the default camera these are
logical window pixels; with a custom camera they are world units. Return to
`set_default_camera()` before drawing a screen-space HUD.

```rust,no_run
use macroquad::prelude::*;

draw_rectangle_rounded(16., 16., 280., 100., 12., Color::new(0.1, 0.2, 0.4, 0.5));
draw_text("Paused", 32., 60., 28., WHITE);
```

Finite nonpositive radii give square corners; positive radii clamp to half
the smaller dimension. Nonfinite inputs, nonpositive dimensions and bounds
that overflow, collapse or invert the tessellation at `f32` precision draw
nothing. Eight segments per
quarter-circle form a convex fan with no overlapping triangle interiors:
translucent fills have a single layer across the interior. Edge antialiasing
depends on framebuffer multisampling. The helper preserves the active camera,
material and clip; it has the same texture-reset behavior as `draw_rectangle`.
It does not add an outline or change built-in widget skins.

Run `cargo run --example rounded_panels` to inspect adjustable radii and
transparency. `rounded_panels_check` shares the native pixel regression and
can also run on WebGL through its accompanying HTML host. The fixture checks
transparent and opaque target coverage, square/clamped radii, invalid inputs
and ordered textured/shape drawing.

## Clip custom document content

`with_clip_rect(rect, || { ... })` clips ordinary immediate shapes, images and
text, intersects nested rectangles and restores the previous clip on return.
The synchronous closure can return a value. A native panic unwind also restores
the previous clip. Empty, disjoint, nonpositive-size and nonfinite rectangles
draw no content; a deeper scope cannot reopen an empty parent.

```rust,no_run
use macroquad::prelude::*;

set_default_camera();
let viewport = Rect::new(20., 60., 280., 180.);
with_clip_rect(viewport, || {
    draw_rectangle(0., 0., 600., 600., WHITE);
    draw_text("Document scrolled above its viewport", 28., 50., 24., BLACK);
    with_clip_rect(Rect::new(40., 80., 80., 40.), || {
        draw_rectangle(0., 0., 600., 600., BLUE);
    });
});
draw_text("Unclipped HUD", 20., 30., 24., WHITE);
```

The clip is an absolute rectangle measured from the whole surface's top-left;
X points right and Y points down. It is independent of the active camera's
world transform. On the window, its units are logical pixels and the current
`screen_dpi_scale()` converts them to physical pixels. On a camera's render
target, its units are texture pixels and window DPI has no effect. The active
camera viewport still uses physical GL coordinates with a bottom-left origin;
the helper converts and intersects that viewport and the surface bounds.
Nested intersections retain precise bounds, then round minima down and maxima
up to physical edges. Thus edge pixels may extend less than one physical pixel
beyond a fractional clip, but logically disjoint nested rectangles stay empty.

Select the camera, render target and viewport before entering the scope and
keep them unchanged until the closure returns. Do not advance a frame inside
the closure. Only immediate drawing is scoped: built-in UI commands render
later with their own clips, while `clear_background` and raw backend calls are
unaffected. Scope entry/exit does not flush geometry; each queued draw retains
its clip, sampler ownership and normal ordering. No `get_internal_gl` call is
needed for a custom document viewport. Zoom, pan and hit testing remain the
application's responsibility.

Run `cargo run --example scoped_clipping` for a scrolling document with nested
image/text clipping and a HUD. `scoped_clipping_check` shares the native GPU
regression; its HTML host runs the same checks on WebGL, including actual
window DPI, an asymmetrical offscreen target/viewport, fractional disjoint
clips, queued texture lifetime, clipped text and deferred built-in UI.

## Wrap and draw text explicitly

`wrap_text` measures words with a chosen font, size and scale and inserts line
breaks for a maximum width. Draw its result with `draw_multiline_text` or
`draw_multiline_text_ex`, and measure it with `measure_multiline_text`.
`draw_text` uses a baseline position; the first multiline baseline is the Y
you pass. Use the same font and scale for wrapping, measuring and drawing.
These operations need a running macroquad context for font textures.

```rust,no_run
use macroquad::prelude::*;

let width = (screen_width() - 32.0).max(1.0);
let wrapped = wrap_text("Pause the game to change these settings.", None, 24, 1.0, width);
draw_multiline_text(&wrapped, 16.0, 80.0, 24.0, Some(1.2), BLACK);
```

The UI `Label::multiline` builder currently stores a value without implementing
multiline layout or width wrapping. Use the text helpers above for a custom
multiline drawing path; reserve its measured space yourself. Text shaping,
fallback fonts and a general UI text layout system remain outside this guide.

## Draw source regions and animations

Load a `Texture2D` once, then use `draw_texture_ex` with
`DrawTextureParams { source: Some(Rect::new(...)), ..Default::default() }` to draw
one source rectangle. Source rectangles use original texture pixels, while
`dest_size` uses the active drawing coordinate space. Rotation is in radians.
An explicit `pivot` is an absolute point in that same drawing space, not an
offset inside the source frame. `flip_x` and `flip_y` mirror the drawn region.
The default pivot is the drawn rectangle's center. Texture batching/atlases
reuse this path; callers still validate sheet dimensions and trimmed-frame
anchors. A checked sprite-region type is not available yet.

[`experimental::animation`](../src/experimental/animation.rs) provides
`Animation`, `AnimatedSprite` and `AnimationFrame` for equal-sized cells with
one animation per sheet row. `frame()` supplies a source rectangle and size
for `draw_texture_ex`; `playing` pauses frame advancement. Its current
`update()` reads `get_frame_time`, advances at most one frame per call, discards
the time remainder after an advance and loops the frame index. It is not a
caller-timed or one-shot player. `set_animation` preserves timing/current frame
modulo the new length; `set_frame` changes the index without resetting time.
Supply nonempty animations with positive tile sizes, frame counts and FPS,
valid indices and an adequately sized texture yourself. Checked construction,
elapsed-time/seek controls and deterministic long-tick handling are queued.
This sprite timer is separate from the completed optional glTF transform and
morph-weight clip player described in the [3D guide](3d-guide.md#playing-gltf-clips).

## Run the existing examples

From the fork root:

```sh
cargo run --example camera
cargo run --example camera_transformations
cargo run --example texture
cargo run --example ui
cargo run --example ui_skins
cargo run --example platformer
```

These upstream examples use existing bundled assets where needed. Use
`cargo build --example <name>` to compile without opening a window, or add
`--target wasm32-unknown-unknown` and use the repository's
[WASM hosting instructions](../README.md#wasm). Serve asset paths over HTTP
when running in a browser. The maintained 2D/UI integration example combining
the proposed helpers remains an unchecked roadmap item.

The renderer batches ordinary geometry; a batch count alone is not a measured
performance result. Default drawing/UI preserve source-over alpha, and queued
text survives font-atlas replacement or font drop; see
[`font_atlas_lifetime`](../examples/README.md#font_atlas_lifetime) for the GPU
fixture. Keep custom material compositing, clip restoration and resource
ownership explicit when mixing drawing paths.
