# UI and 2D guide

This guide describes the APIs available today. Start with the existing
immediate-mode UI, `Camera2D`
and texture drawing; `macroquad-tiled` and `macroquad-platformer` already provide
companion support for tile maps and 2D platformer collisions.

## Choose a coordinate space

| Space | Units and origin | APIs |
| --- | --- | --- |
| Window/UI | Logical window units, top-left origin, X right and Y down | `screen_width`, `screen_height`, `mouse_position_logical`, `touches_logical`, `root_ui` |
| Framebuffer | Physical pixels, top-left origin for the position helpers | `mouse_position_physical`, `touches_physical`, `logical_to_physical`, `physical_to_logical` |
| Drawing target | Physical framebuffer or render-texture pixels, top-left origin, Y down | `Camera2D::world_to_target` / `target_to_world` |
| Camera world | Application-defined world units transformed by the active camera | `Camera2D`, `set_camera`, legacy window-space `screen_to_world`/`world_to_screen` |
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

For a temporary HUD or another camera followed by more world drawing, call
`push_camera_state()` before switching and `pop_camera_state()` afterward.
The stack restores the render pass, viewport, depth mode and matrix, including
the automatic full-surface viewport. Nested pairs restore in reverse order.

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

The legacy `Camera2D::world_to_screen` and `screen_to_world` methods retain
their window-space behavior, including window dimensions when `render_target`
is set. The example above uses the whole window and no render target or viewport.

## Pick a displayed render target

`Camera2D::world_to_target` and `target_to_world` use physical pixels of the
camera's selected drawing surface: the render texture's dimensions, or the
window framebuffer. The origin is the entire surface's top left and Y points
down. Both use the camera's actual matrix, including its different Y orientation
when a render target is selected. A viewport is a physical GL rectangle with a
bottom-left origin; these helpers convert its origin to the surface's top-left
space. A viewport changes where the image is drawn, not the camera zoom.
Window DPI does not scale render-texture pixels.

Choose the conversion path that matches the surface you actually drew:

| Task | Conversion path |
| --- | --- |
| Pick in a directly drawn window camera | `mouse_position_physical()` → `camera.target_to_world` |
| Put a logical HUD label over a world point in that window | `camera.world_to_target` → `physical_to_logical` |
| Pick in a displayed render texture | `mouse_position_logical()` → `window_to_render_target` → `camera.target_to_world` |
| Put a logical HUD label over that render texture | `camera.world_to_target` → `render_target_to_window` |

Use the same camera configuration for drawing and conversion. Changing the
surface size in a `_with_size` call does not select an offscreen camera or
change its Y orientation; the camera's `render_target` field still does that.

For direct window picking, pass `mouse_position_physical()` to
`target_to_world`; convert `world_to_target` results with `physical_to_logical`
when placing a logical HUD. The `_with_size(point, target_size)` alternatives
accept an explicit physical surface size and need no live window. They still
use the camera's selected render-target orientation. Camera conversion
extrapolates outside the surface/viewport. Invalid dimensions, viewports,
nonfinite values or singular camera transforms return `None`.

For an offscreen target drawn into a logical window rectangle, use
`window_to_render_target` before camera unprojection. Its inverse,
`render_target_to_window`, positions overlays on the displayed image. Supply
the entire texture's physical size and the same logical display rectangle and
`flip_x`/`flip_y` flags used by `draw_texture_ex`. For a render-target texture,
`flip_y: true` displays top-left framebuffer Y directly; `false` reverses Y.
The helpers reject points in letterbox bars and outside the full texture.
They support axis-aligned full-texture display; cropped sources and rotated
display quads need a caller-owned mapping.

```rust,no_run
use macroquad::prelude::*;

let target = render_target(320, 180);
let target_size = target.texture.size();
let camera = Camera2D {
    render_target: Some(target.clone()),
    rotation: 27.,
    viewport: Some((17, 11, 280, 145)),
    ..Camera2D::from_display_rect(Rect::new(0., 0., 320., 180.))
};
// Recompute this rectangle whenever the logical window size changes.
let scale = (screen_width() / target_size.x).min(screen_height() / target_size.y);
let size = target_size * scale;
let display = Rect::new((screen_width() - size.x) * 0.5,
                        (screen_height() - size.y) * 0.5, size.x, size.y);
let picked = window_to_render_target(mouse_position_logical(), target_size,
                                     display, false, true)
    .and_then(|point| camera.target_to_world(point));
set_camera(&camera);
clear_background(BLACK);
draw_rectangle(196., 52., 28., 20., SKYBLUE);
if let Some(point) = picked {
    draw_circle(point.x, point.y, 3., YELLOW);
}
set_default_camera();
draw_texture_ex(&target.texture, display.x, display.y, WHITE,
    DrawTextureParams { dest_size: Some(size), flip_y: true, ..Default::default() });
// The label is ordinary logical-window drawing over the displayed world point.
if let Some(label) = camera.world_to_target(vec2(210., 62.))
    .and_then(|point| render_target_to_window(point, target_size, display, false, true))
{
    draw_text("Sprite", label.x, label.y - 12., 20., WHITE);
}
```

Create the target once, keep it with the camera, and repeat the mapping/drawing
part each frame. Recompute the letterbox rectangle after resize. Display-mapping
helpers reject the outer letterbox bars, but do not know the camera's viewport;
if only a sub-viewport should be interactive, test that target-pixel rectangle
before unprojecting. Camera projection/unprojection itself deliberately
extrapolates outside it.

Run `cargo run --example camera_target` and click the cyan sprite after resizing
between 390 x 400 and desktop dimensions. Space flips the displayed Y axis,
X flips X, and R changes camera rotation. The shared
[`scene`](../examples/support/camera_target_scene.rs) shows the complete mapping.
`camera_target_check` checks actual target/window pixel colors for off-center
markers, rotation, nonzero viewports, nested camera/HUD restoration and all
display flip combinations, then
leaves the same live scene available for native or real browser pointer input.
Its HTML host supports DPR 1/2 and resize checks without external assets.

## Build menus with the existing UI

Submit widgets every frame; store the game/settings values outside the widgets.
Keep IDs stable and unique for stateful windows, groups and editboxes, using
`macroquad::ui::hash` (or your own `u64` IDs). Repeated generated widgets need an
item-specific hash input. Existing button calls retain their pointer behavior. Add a stable `.id(id)` to
opt a button into keyboard focus, shared with editboxes as described below.

Borrow `root_ui()` once while building a menu, then use the `&mut Ui` supplied
to each contents closure. Calling `root_ui()` again while that borrow is live
causes a runtime borrow panic. End the borrow before `next_frame().await`;
the next frame processes input and completes the deferred UI drawing.

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
        widgets::Button::new("Resume").id(hash!("resume"))
            .size(vec2(width, 36.)).ui(ui);
        widgets::Group::new(hash!("options"), ui.available_size())
            .layout(Layout::Vertical)
            .content_layout(LayoutSettings::new(Layout::Vertical)
                .padding(RectOffset::new(8., 8., 8., 16.))
                .gap(vec2(0., 6.)))
            .ui(ui, |ui| {
                for option in 0..12 {
                    widgets::Button::new(format!("Option {}", option + 1))
                        .id(hash!("option", option))
                        .size(vec2(ui.available_width(), 32.)).ui(ui);
                }
            });
    });
```

`Ui::available_size`/`available_width` account for padding, the current flow,
indentation and a visible vertical scrollbar. Recompute widths each frame,
including the width passed to `Label::wrap`. A wrapped label reserves its
measured multiline height as one child; ordinary label calls retain their
single-line behavior. The [text section](#wrap-and-draw-text-explicitly) defines
spacing, margins and measurement for caller-owned layouts.
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

This GPU-free example reserves two custom HUD buttons, so the same geometry
can drive drawing and pointer hit testing:

```rust
use macroquad::prelude::*;
use macroquad::ui::{Layout, LayoutCursor, LayoutSettings};

let mut row = LayoutCursor::new(Rect::new(16., 16., 280., 40.),
    LayoutSettings::new(Layout::Horizontal).gap(vec2(8., 0.)));
let back = row.allocate(vec2(80., 40.));
let next = row.allocate_width(40.);
assert_eq!(next, Rect::new(104., 16., 192., 40.));
assert!(back.contains(vec2(20., 20.)));
```

`LayoutCursor` only allocates rectangles. Custom drawing still needs its own
clipping, input handling and keyboard focus; use widgets when those built-in
behaviors are needed.

Run `cargo run --example responsive_layout` and resize to 390 x 400. The
pause/settings example uses wrapped text, an oversized scroll group and a
pinned action row. `responsive_layout_check` shares the native/WebGL fixture
for resize, geometry, clipping, scroll reachability and pointer input. Clipped
buttons do not react to pointer events outside their group's visible content.

## Make buttons usable from the keyboard

Use `widgets::Button::new(label).id(stable_id)` for a keyboard-operable button.
The ID identifies the action, so keep it unchanged when its label, position or
value changes and keep it unique across the `Ui`. Existing `Ui::button` and
`Button::new` calls without an ID remain pointer-only. Pointer activation still
works on identified buttons and focuses that button.

```rust,no_run
use macroquad::prelude::*;
use macroquad::ui::{hash, root_ui, widgets};

let mut ui = root_ui();
let can_save = true;
if widgets::Button::new("Save")
    .id(hash!("save-action"))
    .enabled(can_save)
    .focus_color(YELLOW)
    .size(vec2(220., 36.))
    .ui(&mut ui)
{
    // Save once for this pointer click or Enter/Space press.
}
```

Tab and Shift+Tab traverse enabled identified buttons and editboxes in
submission order. Traversal is resolved after the frame's complete widget
registration, so removed or newly disabled controls cannot remain tab stops.
Fully clipped and empty controls are also excluded; use the existing scroll
interaction to reveal an offscreen control before focusing it. Submit the same
current UI each frame. Enter and Space activate a focused
button once per press; repeated key-down events while held do not activate
again. Editboxes keep text input and their existing editing keys. A button's
inset focus outline is blue by default; `.focus_color(color)` can match your
skin without changing existing `Skin` or `Style` struct literals.
`.enabled(false)` suppresses both pointer and keyboard activation.
`Ui::focused_widget()` reports the current ID, and existing `set_input_focus`
and `clear_input_focus` let the caller choose or clear it.

Use the button's returned `bool` as the activation signal. Do not also handle
Enter/Space as the same application's menu action: a global `is_key_pressed`
shortcut is separate from UI routing. Gate gameplay shortcuts while a menu or
modal owns input, and keep a visible focus indicator when customizing colors.

Use `Ui::modal(id, position, size, |ui| { ... })` when a confirmation needs an
explicit position. This uses the same existing single-modal path as
`widgets::Popup`; nested modals are not supported. Submit the modal while it
is open and stop submitting it to close it. Its first enabled focusable
control receives focus, Tab/Shift+Tab stay inside it, and the activation that
opened it does not activate its first button. Closing restores the prior
control when it is still present and enabled. Underlying controls remain
inactive while the modal is active. Positions and sizes use the active UI's
coordinates, logical window units for `root_ui`.

Keep the open state outside the frame's UI construction. The following
function can be called each frame after the opener sets `open`; its return
value carries the confirmed action back to the application:

```rust,no_run
use macroquad::prelude::*;
use macroquad::ui::{hash, widgets, Layout, LayoutSettings, Ui};

fn confirm_reset(ui: &mut Ui, open: &mut bool) -> bool {
    if !*open {
        return false;
    }
    let (reset, cancel) = ui.modal(hash!("reset-modal"), vec2(24., 70.),
        vec2(300., 110.), |ui| {
            ui.set_layout(LayoutSettings::new(Layout::Vertical)
                .padding(RectOffset::new(12., 12., 12., 12.))
                .gap(vec2(0., 8.)));
            let reset = widgets::Button::new("Reset")
                .id(hash!("reset-confirm"))
                .size(vec2(ui.available_width(), 32.)).ui(ui);
            let cancel = widgets::Button::new("Cancel")
                .id(hash!("reset-cancel"))
                .size(vec2(ui.available_width(), 32.)).ui(ui);
            (reset, cancel)
        });
    if reset || cancel {
        *open = false; // stop submitting the modal next frame
    }
    reset
}
```

Run `cargo run --example keyboard_buttons` for a settings menu with an
editbox, disabled save state and a reset confirmation. The native/WebGL
`keyboard_buttons_check` fixture exercises traversal, editbox input, repeated
Space, removal/disabling, modal containment/restoration and legacy pointer
activation. Its browser exports expose counters, focus and widget rectangles
for additional real DOM keyboard/pointer checks; the fixture's scripted public
`InputHandler` calls alone are not OS input. See the slider section below;
gamepad navigation and platform accessibility bridges remain separate work.

## Make checkboxes usable from the keyboard

Use `Checkbox::new(id).keyboard_focus(true)` with a unique stable ID. It shares
Tab/Shift+Tab submission order with editboxes and opted-in buttons, including
modal containment and focus restoration. Space and Enter both toggle once per
press; held-key repeats do not toggle again. `.enabled(false)` blocks pointer
and keyboard input and skips traversal; omitted or fully clipped controls cannot
retain keyboard focus. Traversal applies next frame, like buttons. Scroll a
fully clipped control into view before focusing it.

```rust,no_run
use macroquad::prelude::*;
use macroquad::ui::{hash, root_ui, widgets};
let mut step_assist = false;
widgets::Checkbox::new(hash!("step-assist"))
    .keyboard_focus(true).focus_color(YELLOW)
    .label("Step assist").size(vec2(240., 28.))
    .ui(&mut root_ui(), &mut step_assist);
```

The inset outline surrounds the checkbox square in both checked states, independent
of skin colors. Existing `ui.checkbox` and builder calls remain pointer-only and
toggle on pointer release over the square. Keyboard activation and pointer release
in the same frame produce one toggle. `keyboard_checkboxes_check` runs the shared
public-input fixture and then leaves the menu live for OS/DOM input; its HTML host
supports resize and DPR checks. Gamepad and accessibility bridges remain deferred.

## Make sliders usable from the keyboard

Opt into `Slider::keyboard_focus(true)` and supply `keyboard_step(step)` in value
units. The existing numeric field is the first Tab stop, followed by the track;
checkboxes, buttons and other editboxes share the same stable-ID order and modal
rules. Both slider IDs (the supplied ID and its derived editbox ID) must remain
unique in the `Ui`. Existing calls keep pointer dragging and numeric text editing.

```rust,no_run
use macroquad::{prelude::*, ui::{hash, root_ui, widgets}};
let mut volume = 0.5;
widgets::Slider::new(hash!("volume"), 0.0..1.0)
    .keyboard_focus(true).keyboard_step(0.1)
    .focus_color(YELLOW).label("Volume")
    .ui(&mut root_ui(), &mut volume);
```

Right/Up increase and Left/Down decrease once per fresh press, with no held-key
repeat. Home selects the exact start and End the exact end, including the normally
exclusive range end. Opposing arrow directions cancel; two arrows in the same
direction count once. End takes precedence over Home, then arrows. Ctrl-modified
keys have no track action. Values clamp before stepping, compute in f64 and round
to f32, so a step smaller than local f32 resolution can leave the value unchanged.

For opted-in sliders, nonfinite values/endpoints, nonascending ranges and missing,
nonfinite or nonpositive steps disable all input without changing caller data.
If the label and numeric field leave no finite positive track width, the track
skips pointer input and Tab traversal. Its numeric field remains editable when
otherwise enabled; reserve enough width for a visible track in compact layouts.
`enabled(false)` also disables both Tab stops, numeric editing and dragging.
The separate inset track outline defaults to blue and can use `focus_color`;
fully clipped controls do not enter traversal. After pointer dragging, an opted-in
track keeps focus for keyboard adjustment. Numeric fields retain normal text
cursor keys, independently of track actions. During an active drag, pointer
position wins over a keyboard adjustment in the same frame.

[`keyboard_sliders_check`](../examples/keyboard_sliders_check.rs) runs the shared
public-input checks and leaves a compact settings menu live for native/DOM input.
The [HTML host](../examples/keyboard_sliders_check.html) uses the same fixture.
Gamepad navigation and accessibility bridges remain deferred.

For a mixed settings panel, keep values outside the frame function, borrow the
UI once and submit controls in the desired traversal order:

```rust,no_run
use macroquad::{prelude::*, ui::{hash, widgets, Ui}};

fn settings(ui: &mut Ui, assist: &mut bool, volume: &mut f32, editable: bool) {
    widgets::Checkbox::new(hash!("settings-assist"))
        .keyboard_focus(true).enabled(editable).focus_color(YELLOW)
        .label("Step assist").ui(ui, assist);
    widgets::Slider::new(hash!("settings-volume"), 0.0..1.0)
        .keyboard_focus(true).keyboard_step(0.05).enabled(editable)
        .focus_color(YELLOW).label("Volume").ui(ui, volume);
    widgets::Button::new("Apply").id(hash!("settings-apply"))
        .enabled(editable).ui(ui);
}
```

The order is checkbox, numeric field, track, then Apply. Arrow keys edit text
while the numeric field owns focus and adjust the value while the track owns
focus. Treat the returned button activation as an application action when
using this recipe; gate global shortcuts separately while the menu is open.
The `keyboard_sliders_check` fixture includes this mixed traversal and modal
restoration on native and WebGL; compiling the recipe does not exercise input.

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

`draw_rectangle_rounded_lines(x, y, width, height, radius, thickness, color)`
uses the same outer boundary, with the full stroke width placed inward. Radius
clamps first; the inner radius is `max(radius - thickness, 0)`. Nonpositive
thickness draws nothing; thickness at least half the smaller dimension fills
the outer shape. Nonfinite geometry, empty dimensions, unrepresentable insets
and bounds that collapse or invert at f32 precision draw nothing. Existing
`draw_rectangle_lines` keeps its established behavior.

```rust,no_run
use macroquad::prelude::*;
draw_rectangle_rounded_lines(16., 16., 280., 100., 12., 3., SKYBLUE);
```

The outline shares vertices for single translucent coverage, follows the current
camera and respects `with_clip_rect` without changing camera/material/clip state.
It does not automatically style widgets. `rounded_outlines_check` compares
4,718,592 native/WebGL pixels against separate fill masks, including transformed
cameras, nested clipping, square/clamped corners and empty/full-width strokes.

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

A render clip does not suppress input. Intersect custom hitboxes with the
visible viewport before accepting a click, and convert the pointer into the
same space as the hitbox. Use logical input for a window-space document or the
[displayed-target mapping](#pick-a-displayed-render-target) for an offscreen
document. A built-in `Group` handles its own widget clipping and hit testing;
it does not clip ordinary draw calls placed inside its contents closure.

Run `cargo run --example scoped_clipping` for a scrolling document with nested
image/text clipping and a HUD. `scoped_clipping_check` shares the native GPU
regression; its HTML host runs the same checks on WebGL, including actual
window DPI, an asymmetrical offscreen target/viewport, fractional disjoint
clips, queued texture lifetime, clipped text and deferred built-in UI.

## Zoom and pan a document

Run `cargo run --example document_zoom_pan` for an asset-free, generated page
that combines `Camera2D`, `with_clip_rect` and logical pointer input. The
[example](../examples/document_zoom_pan.rs) keeps its interaction state in the
application. Page coordinates start at the top left, with Y down. Its transform
is `window = viewport_origin + pan + page * zoom`; `pan` is a logical offset
inside the viewport and `zoom` is logical window units per page unit.

Invert the same transform for picking. To zoom around the pointer, save the
page point beneath it, change zoom, and solve for the new pan:

```rust
use macroquad::prelude::*;

let viewport_origin = vec2(20., 120.);
let pan = vec2(-100., -200.);
let zoom = 1.0;
let pointer = vec2(180., 240.); // mouse_position_logical() in a live frame
let page_point = (pointer - viewport_origin - pan) / zoom;
let next_zoom = 1.25;
let next_pan = pointer - viewport_origin - page_point * next_zoom;
assert!((viewport_origin + next_pan + page_point * next_zoom - pointer).length() < 0.001);
```

Clamp pan after zoom, dragging and scrolling: center a page axis smaller than
the viewport, and otherwise keep that axis between `viewport_extent -
page_extent * zoom` and zero. At a bound, this policy can move the saved
pointer anchor so the page cannot be dragged away. The example's headless
tests cover the inverse mapping, anchor preservation when unconstrained,
bounds and resize policy.

Wheel input and page hit testing first check the visible logical viewport;
the render clip alone does not gate them. Draw with the matching page camera
selected before `with_clip_rect(viewport, ...)`, then restore the default
camera for the header and footer after the scope. Keep input, pan and the
clip in logical units at every DPI; the window clip helper converts to
framebuffer pixels.

The [example index](../examples/README.md#document_zoom_pan) lists the controls,
fit/reset and resize behavior, and the build/copy steps for its native and
[WebGL host](../examples/document_zoom_pan.html). Inspect real input at
390 x 400 and desktop sizes at browser DPR 1 and 2, including wheel/clicks
outside the viewport and document edges crossing beneath the restored HUD.

## Wrap and draw text explicitly

`wrap_text` measures words with a chosen font, size and scale and inserts line
breaks for a maximum width. Draw its result with `draw_multiline_text` or
`draw_multiline_text_ex`, and measure it with `measure_multiline_text`.
`draw_text` uses a baseline position; the first multiline baseline is the Y
you pass. Use the same font, font size and scale for wrapping, measuring and
drawing, including a custom UI label's skin. Measuring at one size and drawing
at another can clip lines even when the enclosing layout rectangle is correct.
These operations need a running macroquad context for font textures.

```rust,no_run
use macroquad::prelude::*;

let width = (screen_width() - 32.0).max(1.0);
let wrapped = wrap_text("Pause the game to change these settings.", None, 24, 1.0, width);
draw_multiline_text(&wrapped, 16.0, 80.0, 24.0, Some(1.2), BLACK);
```

For a built-in UI label, opt into `Label::wrap(outer_width)`. The width is in
logical window units and includes the label skin's content and background
margins. The same skin font, font size and current DPI raster size are used
for wrapping, measurement and drawing. Lines start at the inner left edge and
use existing group clipping; a narrow label also clips its own content width.
Row/column layout and group scroll extents receive the measured full height.

```rust,no_run
use macroquad::prelude::*;
use macroquad::ui::{root_ui, widgets::Label};

let mut ui = root_ui();
let width = ui.available_width(); // recompute each frame after resize
Label::new("A longer settings description.\nAn explicit second paragraph.")
    .wrap(width).multiline(22.).ui(&mut ui);
```

`multiline(spacing)` specifies absolute baseline spacing in logical units only
when wrapping is selected. Its default is the font size; nonfinite or
nonpositive spacing uses that default. Smaller positive spacing deliberately
overlaps lines, while measured height still contains their glyphs. `\n` and
CRLF are hard breaks, preserving empty and trailing lines. Empty text reserves
one font-size line. The existing word/character wrapping splits long words;
each glyph wider than the available content gets its own clipped line, without
synthetic empty lines. Whitespace follows the existing `wrap_text` behavior.

Combined content/background margin sides that are negative or nonfinite
become zero on this opt-in path. A positive outer width exhausted by margins
reserves one empty line plus vertical margins; a nonfinite/nonpositive outer
width reserves zero and draws nothing. `size(vec2(x, y))` supplies a minimum
height for a wrapped label; its X is ignored because `wrap` owns the width.
An invalid outer width also leaves the row/column cursor and its gaps untouched;
a valid empty label still reserves its line and participates in layout.
Without `wrap`, `multiline` retains its historical no-op behavior and `size`
retains its previous meaning.

`Label::measure(&mut ui)` returns the exact allocation without advancing the
cursor or drawing. It needs a running context and uses the active skin. For a
caller-owned `LayoutCursor`, measure the same builder before allocating. Use
container-local rectangle positions relative to the active container's padded
origin for `Label::position` (root coordinates when placing in the root UI);
rebuild both after resize.
Measurement can populate the font glyph cache even though it does not draw.
Keep the same builder, skin and width for allocation and drawing:

```rust,no_run
use macroquad::prelude::*;
use macroquad::ui::{root_ui, widgets::Label, Layout, LayoutCursor, LayoutSettings};

let mut ui = root_ui();
let mut column = LayoutCursor::new(Rect::new(16., 16., 240., 300.),
    LayoutSettings::new(Layout::Vertical).gap(vec2(0., 8.)));
let description = Label::new("A description that grows to its full wrapped height.")
    .wrap(column.available_size().x);
let rect = column.allocate(description.measure(&mut ui));
description.position(rect.point()).ui(&mut ui);
let next_control = column.allocate_width(32.);
draw_rectangle_lines(next_control.x, next_control.y, next_control.w, next_control.h, 1., WHITE);
```

Run `cargo run --example wrapped_labels_check` for a compact/desktop menu and
shared native/WebGL checks with asymmetric margins, a custom font, tight/narrow
geometry, measured scroll extents and clipped large text. The
[WebGL host](../examples/wrapped_labels_check.html) exposes live geometry for
real pointer/wheel checks at DPR 1 and 2. Text shaping, fallback fonts, IME and
a general UI text layout system remain outside this guide.

## Draw source regions and animations

Load a `Texture2D` once, then use `draw_texture_ex` with
`DrawTextureParams { source: Some(Rect::new(...)), ..Default::default() }` to draw
one source rectangle. Source rectangles use original texture pixels, while
`dest_size` uses the active drawing coordinate space. Rotation is in radians.
An explicit `pivot` is an absolute point in that same drawing space, not an
offset inside the source frame. `flip_x` and `flip_y` mirror the drawn region.
The default pivot is the drawn rectangle's center. Texture batching/atlases
reuse this path.

For validated frame metadata, use `SpriteRegion` from
`experimental::animation`. `new` checks an explicit source rectangle;
`from_grid` selects a zero-based column and row from equal-sized cells.
`trimmed` also records the original untrimmed frame size and the visible
rectangle's offset inside it. Dimensions and offsets use original source
pixels, with X right and Y down. Empty, nonfinite, out-of-sheet regions and
inconsistent trimmed bounds return `SpriteRegionError`.

| Stored sheet data | Constructor | Frame size before drawing |
| --- | --- | --- |
| Equal-sized cells without gaps | `SpriteRegion::from_grid` | Cell size |
| Arbitrary complete frame rectangles | `SpriteRegion::new` | Source rectangle size |
| Frames with transparent margins removed | `SpriteRegion::trimmed` | Explicit original frame size |

The source rectangle locates pixels **on the sheet**. The trim offset and pivot
locate them **inside the original frame**; they do not include the source
rectangle's sheet origin. Construct regions when loading sheet metadata, then
reuse them when selecting or drawing frames.

```rust,no_run
use macroquad::experimental::animation::{SpriteRegion, SpriteRegionDrawParams};
use macroquad::prelude::*;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let sheet = load_texture("character.png").await?;
let frame = SpriteRegion::trimmed(
    sheet.size(),
    Rect::new(0., 0., 12., 16.), // stored visible pixels
    vec2(24., 24.),             // original frame before trimming
    vec2(6., 8.),               // visible pixels' original offset
)?.with_pivot(vec2(12., 24.))?; // feet anchor, in original frame pixels
frame.draw(&sheet, vec2(100., 160.), WHITE, SpriteRegionDrawParams {
    dest_size: Some(vec2(48., 48.)), // size of the whole untrimmed frame
    flip_x: true,
    ..Default::default()
})?;
# Ok(())
# }
```

`with_pivot` chooses the source point placed at the drawing-space anchor;
the default is the untrimmed frame's top-left. It may lie outside the frame.
Flips reflect the image around that fixed anchor, including the trimmed
offset; rotation then turns the result around the same anchor. This differs
from flipping a raw `DrawTextureParams` rectangle in place. Positive
`dest_size` scales the whole untrimmed frame, so differently trimmed frames
keep the same scale. A non-centered pivot therefore needs no special-case
translation when changing frames or flips.

Regions are GPU-free metadata and do not own a texture. `draw` borrows a
`Texture2D`, checks its original dimensions against the sheet size, and uses
`draw_texture_ex`, retaining existing texture ownership, atlas batching,
clipping and material behavior. `draw_params` provides the same checked
position/parameters without a GPU; callers using that lower-level path must
check the supplied texture dimensions themselves. Neither path changes
animation timing or imports a packed-atlas format.

Run `cargo run --example sprite_regions` to inspect the tiny generated sheet.
`sprite_regions_check` and its HTML host share the native/WebGL regression.

[`experimental::animation`](../src/experimental/animation.rs) provides
`Animation`, `AnimatedSprite` and `AnimationFrame` for equal-sized cells with
one animation per sheet row. Existing struct literals are unchanged.
`try_new` checks nonzero tile dimensions, a nonempty animation list and positive
frame counts/FPS. Texture bounds are checked separately with `SpriteRegion`.

### Control the sprite clock

Pass elapsed **seconds** to `update_with_delta(f64)` from your simulation clock.
It advances all elapsed frames and keeps fractional time; `update()` is a
convenience using `get_frame_time()`. A frame boundary belongs to the next frame.
Finite nonnegative times are accepted; negative or nonfinite times return an
error without changing the player. Floating-point precision still applies:
arbitrary decimal time partitions are not promised to be bit-identical.

```rust
use macroquad::experimental::animation::{
    AnimatedSprite, Animation, AnimationPlayback, SpriteRegion,
};
use macroquad::prelude::*;

let clips = [Animation { name: "pulse".into(), row: 0, frames: 2, fps: 2 }];
let mut sprite = AnimatedSprite::try_new(20, 20, &clips, true).unwrap();
sprite.update_with_delta(2.75).unwrap(); // two loops plus 0.75 seconds
assert_eq!(sprite.current_frame(), 1);
sprite.playing = false;
sprite.update_with_delta(100.0).unwrap(); // pause preserves the fraction
sprite.seek(0.25).unwrap(); // seeking also works while paused
sprite.set_playback(AnimationPlayback::Once);
sprite.playing = true;
sprite.update_with_delta(0.75).unwrap();
assert!(sprite.is_finished()); // last frame has had its full duration
let region = SpriteRegion::new(vec2(40., 20.), sprite.frame().source_rect).unwrap();
assert_eq!(region.source(), Rect::new(20., 0., 20., 20.));
sprite.reset(); // first frame, zero elapsed time, completion cleared
assert!(!sprite.is_finished());
```

`AnimationPlayback::Loop` is the default. `Once` clamps at the clip's duration
and holds the last frame. `is_last_frame()` reports which frame is visible;
`is_finished()` reports completion after that frame's full interval.
Completion does not change the caller's `playing` flag. `reset()` and `seek()`
also preserve it, so resetting a completed player resumes only if it is playing.
`elapsed()` is the wrapped loop position or the clamped one-shot position;
`duration()` is the frame count divided by FPS. Each instance owns its clock.
For unequal frame durations, use `SpriteClip` and `TimedSprite` beside this
uniform-FPS API. A clip owns checked `SpriteRegion` values and individual `f64`
seconds; it owns no texture. `Animation` and `AnimationFrame` literals and
`AnimatedSprite` behavior stay unchanged.

| Player | Clip metadata | Clock and selection |
| --- | --- | --- |
| `AnimatedSprite` | Equal grid cells with one integer FPS per row-based `Animation` | `update_with_delta` or frame-time `update`; `set_animation` selects a stored clip |
| `TimedSprite` | An immutable `SpriteClip` of checked regions and individual seconds | `update_with_delta` only; construct a new player to replace its clip |

Both players own independent elapsed time, pause state and loop/once policy.
They share no texture ownership; supply the sheet that matches the current
region when drawing. Configure the mode and pause state explicitly when
creating a replacement `TimedSprite`; a new player starts at zero in Loop.

```rust
use macroquad::experimental::animation::{SpriteClip, TimedSprite, SpriteRegion};
use macroquad::prelude::*;
let anticipation = SpriteRegion::from_grid(vec2(40., 20.), vec2(20., 20.), 0, 0).unwrap();
let recovery = SpriteRegion::from_grid(vec2(40., 20.), vec2(20., 20.), 1, 0).unwrap();
let clip = SpriteClip::new(&[(anticipation, 0.125), (recovery, 0.375)]).unwrap();
let mut player = TimedSprite::new(clip, true);
player.update_with_delta(0.125).unwrap();
assert_eq!(player.current_frame(), 1); // exact boundary selects the next region
assert_eq!(player.duration(), 0.5); // sum, including the held recovery frame
```

Empty clips, nonfinite/nonpositive durations, overflowing sums and additions
that cannot advance the accumulated boundary are rejected. Each region occupies
`[start, end)` using stored `f64` boundaries. Looping wraps at the total;
`Once` holds the final region and finishes at that total. Seek, pause, reset,
mode changes and invalid-time handling follow the same policy as the uniform
player. Large deltas reduce whole cycles before addition; fractional progress
is retained. Floating-point rounding applies, so differently subdivided clocks
need not match bit for bit. Draw `player.frame()` through `SpriteRegion::draw`
with its matching texture, preserving trim/pivot/flip behavior. The maintained
`timed_sprite_check` example uses the tiny asymmetric sheet and checks actual
pixels on native/WebGL before showing interactive loop/once playback.

Choose the operation by the state change you want. Except for clip selection,
these lifecycle operations apply to both players:

| Action | Operation | Result |
| --- | --- | --- |
| Pause and resume | Change `playing` | Keep the current frame and fractional progress |
| Restart the selected clip | `reset()` | Rewind and clear completion; preserve `playing` and playback mode |
| Jump to an absolute clip time | `seek(seconds)` | Wrap a loop or clamp a one-shot, even while paused |
| Select a uniform-FPS clip and start it fresh | `AnimatedSprite::set_animation(index)`, then `reset()` | Explicitly discard the previous clip's progress |
| Replay a completed one-shot | `reset()`, then set `playing = true` | Start again with the same one-shot policy |

For `AnimatedSprite`, the legacy `set_animation` retains the frame modulo the new length and the
fractional seconds; `set_frame` retains fractional seconds. Both clear
completion. Changing FPS can leave excess fractional seconds pending until the
next playing update, so `elapsed()` is temporarily unnormalized after that
legacy operation. Prefer `reset` or `seek` for an explicit timeline position.
Validate a selected clip index before passing it to `set_animation`; an invalid
index panics without changing the player. `set_frame` is an unchecked legacy
setter, so supply a frame below the selected clip's length. Checked construction
validates animation metadata, while `SpriteRegion` validates the resulting
source rectangle against the actual sheet dimensions before drawing.

Advance each player from one clock: call `update_with_delta` for caller time.
`AnimatedSprite` also offers `update` for frame time; calling both in the same
tick advances it twice. `TimedSprite` has no automatic frame-time method.
For repeatable playback, replay the same sequence of elapsed values and
selection/seek operations; the timer does not choose fixed simulation steps.

Run `cargo run --example sprite_animation` for independently timed loop/once
players. Space plays/pauses, R resets, S seeks to 0.5 seconds and T supplies a
2.75-second tick while playing. `sprite_animation_check` and its HTML host run
the same 72,000-pixel native/WebGL fixture before leaving the demo interactive.
It reuses the generated sheet from `sprite_regions`; the player still owns no
texture and drawing uses the existing checked source-region path.

This sprite timer is separate from the completed optional glTF transform and
morph-weight clip player described in the [3D guide](3d-guide.md#playing-gltf-clips).

## Combine sprites, picking and a pause menu

Run `cargo run --example sprite_ui` for a small complete example of these APIs
working together. It uses the same generated 40 x 20 sheet as `sprite_regions`
and `sprite_animation`; no external assets or optional features are needed.
The [shared scene](../examples/support/sprite_ui_scene.rs) owns the animation,
texture, render target, score and pause state. The executable supplies elapsed
seconds and repeats the scene each frame.

The frame has four responsibilities:

1. Recompute the logical HUD and letterbox rectangles from the current window
   size. Keep the offscreen target's pixel size independent of window DPI.
2. Build the HUD and any pause modal using one scoped `root_ui()` borrow.
   Stable button IDs provide pointer activation, Tab/Shift+Tab traversal and
   Enter/Space activation. The modal contains focus and restores the previously
   focused control when it is still present and enabled.
3. Gate world picking and the animation clock with application pause state.
   Map a logical pointer through `window_to_render_target`, then
   `Camera2D::target_to_world`, and test the sprite's world rectangle. A clip or
   modal alone does not pause the application. Opening or closing a menu must
   consume that frame's world action so the same press cannot also pick a sprite.
4. Draw the checked animation region into the camera's render target, return
   to `set_default_camera()`, and display it in the same letterbox rectangle used
   for picking. Draw the rounded log panel and clip its text in logical window units;
   built-in UI widgets are submitted at the end of the frame with their own clips.

Pause preserves the sprite's fractional clock. Resume continues it, while the
modal's restart action resets the score and animation while staying paused.
The shared scene captures `was_paused` before processing the menu and uses
`was_paused || paused` afterward to block **both** world input and elapsed time.
That blocks the opening and closing frames as well as frames spent in the modal.
Resume starts advancement on the following frame, without replaying the time
spent paused. Apply the same gate to a gameplay timer or 3D movement if one is
added to the scene.
Escape opens or closes the menu. Keep the `Texture2D` and
render target in the scene instead of reloading them each frame. Region
metadata and the animation player do not own textures; queued drawing retains
the texture handles it needs until submission.

Resize between 390 x 400 and desktop dimensions, click the visible sprite, and
open the pause menu with the HUD button. Try keyboard traversal and activation,
clicking the world behind the modal, and resuming after a delay. The score and
clock should stop during pause, the controls should remain reachable, and the
log should stay inside its panel without clipping the HUD.

`sprite_ui_check` and its HTML host run the shared native/WebGL fixture before
leaving the same scene interactive. They exercise the combined flow and queued
texture lifetime. Actual native and browser input checks complement the
fixture's scripted public UI input. Build/hosting commands are in the
[example index](../examples/README.md#ui-and-2d-learning-path).

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
when running in a browser. Use `sprite_ui` above for a maintained example
combining the completed sprite, camera and UI helpers.

The renderer batches ordinary geometry; a batch count alone is not a measured
performance result. Default drawing/UI preserve source-over alpha, and queued
text survives font-atlas replacement or font drop; see
[`font_atlas_lifetime`](../examples/README.md#font_atlas_lifetime) for the GPU
fixture. Keep custom material compositing, clip restoration and resource
ownership explicit when mixing drawing paths.
