//! Pointer capture state.
//!
//! `G` asks for capture. In a browser that request stays wanted until `Tab`,
//! and the next click grants it inside the pointer event. Escape is left to
//! the browser, so a locked page reports `CursorCapture::Lost` until the
//! next click. `Q` quits.

use macroquad::prelude::*;

fn label(state: CursorCapture) -> &'static str {
    match state {
        CursorCapture::Idle => "idle",
        CursorCapture::Requested => "requested",
        CursorCapture::Active => "active",
        CursorCapture::Denied => "denied",
        CursorCapture::Lost => "lost",
    }
}

#[macroquad::main("Cursor capture")]
async fn main() {
    // Arm once. Desktop confirms immediately. A browser waits for a click.
    set_cursor_grab(true);

    loop {
        if is_key_pressed(KeyCode::Q) {
            break;
        }
        if is_key_pressed(KeyCode::Tab) {
            set_cursor_grab(false);
        }
        if is_key_pressed(KeyCode::G) {
            set_cursor_grab(true);
        }

        clear_background(DARKGRAY);
        let state = cursor_capture();
        draw_text(
            format!("capture: {}", label(state)),
            24.0,
            48.0,
            32.0,
            if state == CursorCapture::Active {
                GREEN
            } else {
                WHITE
            },
        );
        for (index, event) in cursor_capture_events().iter().enumerate() {
            draw_text(
                format!("event: {}", label(*event)),
                24.0,
                96.0 + index as f32 * 28.0,
                24.0,
                YELLOW,
            );
        }
        draw_text(
            "G arms capture. Click grants it in a browser. Tab releases. Q quits.",
            24.0,
            screen_height() - 64.0,
            22.0,
            LIGHTGRAY,
        );
        draw_text(
            "Browser Escape reports lost and does not lock again until the next click.",
            24.0,
            screen_height() - 32.0,
            22.0,
            LIGHTGRAY,
        );

        next_frame().await;
    }
}
