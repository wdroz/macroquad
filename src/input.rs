//! Cross-platform mouse, keyboard (and gamepads soon) module.
//!
//! [`set_cursor_grab`] asks the platform to capture the cursor.
//! [`cursor_capture`] reports whether that request is idle, in flight, active,
//! denied, or lost, and [`cursor_capture_events`] lists the states entered
//! since the previous frame.

use std::collections::HashSet;

pub use crate::cursor_capture::CursorCapture;
use crate::prelude::screen_height;
use crate::prelude::screen_width;
use crate::Vec2;
use crate::{get_context, DroppedFile};
pub use miniquad::{KeyCode, MouseButton};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TouchPhase {
    Started,
    Stationary,
    Moved,
    Ended,
    Cancelled,
}

impl From<miniquad::TouchPhase> for TouchPhase {
    fn from(miniquad_phase: miniquad::TouchPhase) -> TouchPhase {
        match miniquad_phase {
            miniquad::TouchPhase::Started => TouchPhase::Started,
            miniquad::TouchPhase::Moved => TouchPhase::Moved,
            miniquad::TouchPhase::Ended => TouchPhase::Ended,
            miniquad::TouchPhase::Cancelled => TouchPhase::Cancelled,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Touch {
    pub id: u64,
    pub phase: TouchPhase,
    pub position: Vec2,
}

/// Ask the platform to capture or release the mouse cursor.
///
/// `true` confines the cursor on desktop and requests Pointer Lock in the
/// browser. `false` releases it. The frame loop does not repeat this call.
/// Read [`cursor_capture`] for the result: desktop confirms in this call,
/// while a browser may stay [`CursorCapture::Requested`] and later report
/// [`CursorCapture::Denied`] or [`CursorCapture::Lost`]. A denied or lost
/// browser lock does not retry on its own. While this `true` request is still
/// in force, the next mouse or touch press asks once inside that event.
/// Calling this again without such a press does not ask the browser.
///
/// ```no_run
/// use macroquad::prelude::*;
///
/// #[macroquad::main("Look")]
/// async fn main() {
///     set_cursor_grab(true);
///     loop {
///         if is_key_pressed(KeyCode::G) {
///             set_cursor_grab(true);
///         }
///         if is_key_pressed(KeyCode::Tab) {
///             set_cursor_grab(false);
///         }
///         if cursor_capture() == CursorCapture::Active {
///             let _look = mouse_delta_position();
///         }
///         next_frame().await;
///     }
/// }
/// ```
pub fn set_cursor_grab(grab: bool) {
    #[cfg(target_arch = "wasm32")]
    {
        if browser_capture_ready() {
            unsafe { mq_capture_set_wanted(grab as i32) };
            // miniquad applies the grab on the next browser frame. The bundle
            // wrapper drops a duplicate or ungated request.
            miniquad::window::set_cursor_grab(grab);
            sync_cursor_capture();
        } else {
            let context = get_context();
            context.cursor_grabbed = grab;
            context.capture.legacy_set(grab);
            miniquad::window::set_cursor_grab(grab);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let context = get_context();
        context.capture.set_wanted(grab);
        apply_desktop_capture(context);
    }
}

/// Current pointer-capture state.
///
/// See [`CursorCapture`] for the desktop and browser meanings. The value stays
/// at the last report until the next [`set_cursor_grab`] or a platform event.
///
/// ```no_run
/// use macroquad::prelude::*;
///
/// # async fn frame() {
/// if cursor_capture() == CursorCapture::Lost {
///     draw_text("Click to capture the mouse", 20.0, 20.0, 24.0, WHITE);
/// }
/// # next_frame().await;
/// # }
/// ```
pub fn cursor_capture() -> CursorCapture {
    get_context().capture.state()
}

/// Capture states entered since the previous frame, oldest first.
///
/// Reading this does not drain it. The list is cleared at the end of the frame,
/// same as the pressed-key sets. A desktop grab contributes `Requested` and
/// then `Active` on the frame of the call.
///
/// ```no_run
/// use macroquad::prelude::*;
///
/// # async fn frame() {
/// for state in cursor_capture_events() {
///     if state == CursorCapture::Denied {
///         draw_text("Browser refused pointer lock", 20.0, 48.0, 24.0, WHITE);
///     }
/// }
/// # next_frame().await;
/// # }
/// ```
pub fn cursor_capture_events() -> Vec<CursorCapture> {
    get_context().capture.events().to_vec()
}

#[cfg(not(target_arch = "wasm32"))]
fn apply_desktop_capture(context: &mut crate::Context) {
    for grab in context.capture.take_platform() {
        miniquad::window::set_cursor_grab(grab);
    }
    context.cursor_grabbed = context.capture.state() == CursorCapture::Active;
}

/// Desktop focus change. The browser plugin owns this transition on wasm.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn note_capture_focus(focused: bool) {
    let context = get_context();
    context.capture.focus_changed(focused);
    apply_desktop_capture(context);
}

/// Copy the browser plugin's state. No-op on desktop and without the plugin.
pub(crate) fn sync_cursor_capture() {
    #[cfg(target_arch = "wasm32")]
    {
        if !browser_capture_ready() {
            return;
        }
        let context = get_context();
        let Some(state) = CursorCapture::from_raw(unsafe { mq_capture_state() }) else {
            return;
        };
        context.capture.observe_browser(state);
        context.cursor_grabbed = state == CursorCapture::Active;
    }
}

#[cfg(target_arch = "wasm32")]
fn browser_capture_ready() -> bool {
    unsafe { mq_capture_ready() == 1 }
}

#[cfg(target_arch = "wasm32")]
extern "C" {
    fn mq_capture_set_wanted(wanted: i32);
    fn mq_capture_state() -> i32;
    fn mq_capture_ready() -> i32;
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
extern "C" fn macroquad_pointer_capture_crate_version() -> u32 {
    1
}

/// Set mouse cursor visibility
pub fn show_mouse(shown: bool) {
    miniquad::window::show_mouse(shown);
}

/// Return mouse position in pixels.
pub fn mouse_position() -> (f32, f32) {
    let context = get_context();

    (
        context.mouse_position.x / miniquad::window::dpi_scale(),
        context.mouse_position.y / miniquad::window::dpi_scale(),
    )
}

/// Return mouse position in range [-1; 1].
pub fn mouse_position_local() -> Vec2 {
    let (pixels_x, pixels_y) = mouse_position();

    convert_to_local(Vec2::new(pixels_x, pixels_y))
}

/// Returns the difference between the current mouse position and the mouse position on the previous frame.
pub fn mouse_delta_position() -> Vec2 {
    let context = get_context();

    let current_position = mouse_position_local();
    let last_position = context.last_mouse_position.unwrap_or(current_position);

    // Calculate the delta
    last_position - current_position
}

/// This is set to true by default, meaning touches will raise mouse events in addition to raising touch events.
/// If set to false, touches won't affect mouse events.
pub fn is_simulating_mouse_with_touch() -> bool {
    get_context().simulate_mouse_with_touch
}

/// This is set to true by default, meaning touches will raise mouse events in addition to raising touch events.
/// If set to false, touches won't affect mouse events.
pub fn simulate_mouse_with_touch(option: bool) {
    get_context().simulate_mouse_with_touch = option;
}

/// Return touches with positions in pixels.
pub fn touches() -> Vec<Touch> {
    get_context().touches.values().cloned().collect()
}

/// Return touches with positions in range [-1; 1].
pub fn touches_local() -> Vec<Touch> {
    get_context()
        .touches
        .values()
        .map(|touch| {
            let mut touch = touch.clone();
            touch.position = convert_to_local(touch.position);
            touch
        })
        .collect()
}

pub fn mouse_wheel() -> (f32, f32) {
    let context = get_context();

    (context.mouse_wheel.x, context.mouse_wheel.y)
}

/// Detect if the key has been pressed once
pub fn is_key_pressed(key_code: KeyCode) -> bool {
    let context = get_context();

    context.keys_pressed.contains(&key_code)
}

/// Detect if the key is being pressed
pub fn is_key_down(key_code: KeyCode) -> bool {
    let context = get_context();

    context.keys_down.contains(&key_code)
}

/// Detect if the key has been released this frame
pub fn is_key_released(key_code: KeyCode) -> bool {
    let context = get_context();

    context.keys_released.contains(&key_code)
}

/// Detect if any key is being pressed
pub fn is_any_key_down() -> bool {
    let context = get_context();
    context.keys_down.len() > 0
}

/// Return the last pressed char.
/// Each "get_char_pressed" call will consume a character from the input queue.
pub fn get_char_pressed() -> Option<char> {
    let context = get_context();

    context.chars_pressed_queue.pop_front()
}

pub(crate) fn get_char_pressed_ui() -> Option<char> {
    let context = get_context();

    context.chars_pressed_ui_queue.pop_front()
}

/// Return the last pressed key.
pub fn get_last_key_pressed() -> Option<KeyCode> {
    let context = get_context();
    // TODO: this will return a random key from keys_pressed HashMap instead of the last one, fix me later
    context.keys_pressed.iter().next().cloned()
}

pub fn get_keys_pressed() -> HashSet<KeyCode> {
    let context = get_context();
    context.keys_pressed.clone()
}

pub fn get_keys_down() -> HashSet<KeyCode> {
    let context = get_context();
    context.keys_down.clone()
}

pub fn get_keys_released() -> HashSet<KeyCode> {
    let context = get_context();
    context.keys_released.clone()
}

/// Clears input queue
pub fn clear_input_queue() {
    let context = get_context();
    context.chars_pressed_queue.clear();
    context.chars_pressed_ui_queue.clear();
    context.mouse_pressed.clear();
    context.keys_pressed.clear();
}

/// Detect if the button is being pressed
pub fn is_mouse_button_down(btn: MouseButton) -> bool {
    let context = get_context();

    context.mouse_down.contains(&btn)
}

/// Detect if the button has been pressed once
pub fn is_mouse_button_pressed(btn: MouseButton) -> bool {
    let context = get_context();

    context.mouse_pressed.contains(&btn)
}

/// Detect if the button has been released this frame
pub fn is_mouse_button_released(btn: MouseButton) -> bool {
    let context = get_context();

    context.mouse_released.contains(&btn)
}

/// Convert a position in pixels to a position in the range [-1; 1].
fn convert_to_local(pixel_pos: Vec2) -> Vec2 {
    Vec2::new(pixel_pos.x / screen_width(), pixel_pos.y / screen_height()) * 2.0
        - Vec2::new(1.0, 1.0)
}

/// Prevents quit
pub fn prevent_quit() {
    get_context().prevent_quit_event = true;
}

/// Detect if quit has been requested
pub fn is_quit_requested() -> bool {
    get_context().quit_requested
}

/// Gets the files which have been dropped on the window.
pub fn get_dropped_files() -> Vec<DroppedFile> {
    get_context().dropped_files()
}

/// Functions for advanced input processing.
///
/// Functions in this module should be used by external tools that uses miniquad system, like different UI libraries. User shouldn't use this function.
pub mod utils {
    use crate::get_context;

    /// Register input subscriber. Returns subscriber identifier that must be used in `repeat_all_miniquad_input`.
    pub fn register_input_subscriber() -> usize {
        let context = get_context();

        context.input_events.push(vec![]);

        context.input_events.len() - 1
    }

    /// Repeats all events that came since last call of this function with current value of `subscriber`. This function must be called at each frame.
    pub fn repeat_all_miniquad_input<T: miniquad::EventHandler>(t: &mut T, subscriber: usize) {
        let context = get_context();

        for event in &context.input_events[subscriber] {
            event.repeat(t);
        }
        context.input_events[subscriber].clear();
    }
}
