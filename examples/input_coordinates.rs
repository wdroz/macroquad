//! Explicit logical/physical mouse and touch coordinates.
//!
//! `cargo run --example input_coordinates` opens the native fixture. On the web,
//! build this example for wasm32-unknown-unknown and use input_coordinates.html.
//! The fixture-only host callback publishes window.inputCoordinates each frame;
//! it is not a library requirement or an application API.

use macroquad::prelude::*;

fn config() -> Conf {
    Conf {
        window_title: "Input coordinates".into(),
        window_width: 800,
        window_height: 600,
        high_dpi: true,
        ..Default::default()
    }
}

fn touch_json(touches: &[Touch]) -> String {
    touches
        .iter()
        .map(|touch| {
            format!(
                "{{\"id\":{},\"phase\":\"{:?}\",\"position\":[{},{}]}}",
                touch.id, touch.phase, touch.position.x, touch.position.y
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(target_arch = "wasm32")]
fn report(json: &str) {
    extern "C" {
        fn input_coordinates_report(pointer: *const u8, length: usize);
    }
    unsafe { input_coordinates_report(json.as_ptr(), json.len()) };
}

#[cfg(not(target_arch = "wasm32"))]
fn report(_json: &str) {}

#[macroquad::main(config)]
async fn main() {
    // Keep the independent mouse and touch streams visible in this fixture.
    simulate_mouse_with_touch(false);
    let mut frame = 0u64;
    loop {
        if is_key_pressed(KeyCode::Q) {
            break;
        }
        if is_mouse_button_pressed(MouseButton::Left) {
            set_cursor_grab(true);
        }
        if is_key_pressed(KeyCode::Escape) {
            set_cursor_grab(false);
        }
        let mouse = mouse_position_logical();
        let physical = mouse_position_physical();
        let delta = mouse_delta_logical();
        let physical_delta = mouse_delta_physical();
        let legacy_delta = mouse_delta_position();
        let logical_touches = touches_logical();
        let physical_touches = touches_physical();

        let json = format!(
            "{{\"version\":1,\"frame\":{},\"width\":{},\"height\":{},\"dpi\":{},\"mouseLogical\":[{},{}],\"mousePhysical\":[{},{}],\"deltaLogical\":[{},{}],\"deltaPhysical\":[{},{}],\"legacyDelta\":[{},{}],\"touchesLogical\":[{}],\"touchesPhysical\":[{}],\"capture\":\"{:?}\"}}",
            frame, screen_width(), screen_height(), screen_dpi_scale(),
            mouse.x, mouse.y, physical.x, physical.y,
            delta.x, delta.y, physical_delta.x, physical_delta.y,
            legacy_delta.x, legacy_delta.y,
            touch_json(&logical_touches), touch_json(&physical_touches), cursor_capture(),
        );
        report(&json);

        clear_background(DARKGRAY);
        draw_rectangle_lines(120.0, 80.0, 80.0, 80.0, 2.0, WHITE);
        draw_circle_lines(160.0, 120.0, 10.0, 2.0, YELLOW);
        draw_circle(mouse.x, mouse.y, 4.0, RED);
        for touch in &logical_touches {
            draw_circle(touch.position.x, touch.position.y, 8.0, GREEN);
        }
        draw_text(
            format!(
                "DPI {:.2} | logical mouse {:.2}, {:.2}",
                screen_dpi_scale(),
                mouse.x,
                mouse.y
            ),
            20.0,
            220.0,
            24.0,
            WHITE,
        );
        draw_text(
            format!(
                "delta {:.2}, {:.2} | physical {:.2}, {:.2}",
                delta.x, delta.y, physical_delta.x, physical_delta.y
            ),
            20.0,
            252.0,
            24.0,
            WHITE,
        );
        draw_text(
            "Target center: logical (160, 120). Touch green; mouse red.",
            20.0,
            292.0,
            20.0,
            WHITE,
        );
        draw_text(
            "Click arms capture; browser may need next click. Escape releases; Q quits.",
            20.0,
            320.0,
            20.0,
            WHITE,
        );
        frame += 1;
        next_frame().await;
    }
}
