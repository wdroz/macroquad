//! Browser canvas sizing, input/text coordinates and paced frame diagnostics.
//!
//! Build for wasm32-unknown-unknown, place canvas_host.wasm beside canvas_host.html
//! and serve the repository root over HTTP. Native builds use ordinary Conf.
use macroquad::prelude::*;

fn config() -> Conf {
    Conf {
        window_title: "Canvas host configuration".into(),
        window_width: 800,
        window_height: 600,
        high_dpi: true,
        platform: miniquad::conf::Platform {
            blocking_event_loop: cfg!(target_arch = "wasm32"),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[cfg(target_arch = "wasm32")]
fn report(json: &str) {
    extern "C" {
        fn canvas_host_report(pointer: *const u8, length: usize);
    }
    unsafe { canvas_host_report(json.as_ptr(), json.len()) };
}

#[cfg(not(target_arch = "wasm32"))]
fn report(_json: &str) {}

#[macroquad::main(config)]
async fn main() {
    simulate_mouse_with_touch(false);
    let mut frame = 0u64;
    loop {
        if is_key_pressed(KeyCode::Q) {
            break;
        }
        let mouse = mouse_position_logical();
        let physical = mouse_position_physical();
        let delta = mouse_delta_logical();
        let touches = touches_logical();
        clear_background(Color::from_rgba(34, 34, 34, 255));
        draw_text(
            "Resize, zoom, or move this window to another display.",
            20.0,
            30.0,
            20.0,
            WHITE,
        );
        draw_circle_lines(123.0, 87.0, 12.0, 2.0, YELLOW);
        draw_circle(mouse.x, mouse.y, 3.0, RED);
        for touch in &touches {
            draw_circle(touch.position.x, touch.position.y, 6.0, GREEN);
        }
        let measured = measure_text("Canvas host text", None, 24, 1.0);
        draw_rectangle_lines(
            40.0,
            180.0 - measured.offset_y,
            measured.width,
            measured.height,
            1.0,
            SKYBLUE,
        );
        let drawn = draw_text("Canvas host text", 40.0, 180.0, 24.0, WHITE);
        draw_text(
            format!(
                "DPI {:.3} | {:.1} x {:.1} logical | frame {frame}",
                screen_dpi_scale(),
                screen_width(),
                screen_height()
            ),
            20.0,
            230.0,
            20.0,
            WHITE,
        );
        draw_text(
            "Browser configuration and requested/effective report: see console.",
            20.0,
            260.0,
            18.0,
            WHITE,
        );
        let touch_json = touches
            .iter()
            .map(|touch| {
                format!(
                    "{{\"id\":{},\"position\":[{},{}]}}",
                    touch.id, touch.position.x, touch.position.y,
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        report(&format!(
            "{{\"version\":1,\"frame\":{frame},\"width\":{},\"height\":{},\"dpi\":{},\"mouseLogical\":[{},{}],\"mousePhysical\":[{},{}],\"deltaLogical\":[{},{}],\"touchesLogical\":[{touch_json}],\"target\":[123,87],\"textTarget\":[40,180],\"textBounds\":[{},{},{}],\"textDrawBounds\":[{},{},{}]}}",
            screen_width(), screen_height(), screen_dpi_scale(), mouse.x, mouse.y,
            physical.x, physical.y, delta.x, delta.y, measured.width, measured.height,
            measured.offset_y, drawn.width, drawn.height, drawn.offset_y,
        ));
        frame += 1;
        next_frame().await;
    }
}
