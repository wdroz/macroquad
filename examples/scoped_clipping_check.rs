//! Native/WebGL pixel regression, shared with tests/scoped_clipping.rs.
#[path = "../tests/support/scoped_clipping_fixture.rs"]
mod fixture;
use macroquad::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
static STATUS: AtomicU32 = AtomicU32::new(0);
static PIXELS: AtomicU32 = AtomicU32::new(0);
#[no_mangle]
pub extern "C" fn scoped_clipping_status() -> u32 {
    STATUS.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn scoped_clipping_pixels() -> u32 {
    PIXELS.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn scoped_clipping_dpi_milli() -> u32 {
    (screen_dpi_scale() * 1000.).round() as u32
}
fn config() -> Conf {
    Conf {
        window_title: "Scoped clipping checks".into(),
        window_width: 512,
        window_height: 320,
        high_dpi: true,
        ..Default::default()
    }
}
#[macroquad::main(config)]
async fn main() {
    std::panic::set_hook(Box::new(|info| {
        STATUS.store(2, Ordering::Relaxed);
        macroquad::logging::error!("SCOPED_CLIPPING_FAIL: {info}");
    }));
    PIXELS.store(fixture::run().await, Ordering::Relaxed);
    STATUS.store(1, Ordering::Relaxed);
    macroquad::logging::info!("SCOPED_CLIPPING_PASS");
    loop {
        clear_background(DARKBLUE);
        draw_text("Scoped clipping: passed", 20., 40., 28., WHITE);
        macroquad::ui::root_ui().button(vec2(20., 70.), "Independent built-in UI");
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        next_frame().await;
    }
}
