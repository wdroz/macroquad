//! Native/WebGL pixel regression; see rounded_panels for the interactive demo.
#[path = "../tests/support/rounded_panels_fixture.rs"]
mod fixture;

use macroquad::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
static STATUS: AtomicU32 = AtomicU32::new(0);
static PIXELS: AtomicU32 = AtomicU32::new(0);

#[no_mangle]
pub extern "C" fn rounded_panels_status() -> u32 {
    STATUS.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn rounded_panels_pixels() -> u32 {
    PIXELS.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn rounded_panels_dpi_milli() -> u32 {
    (screen_dpi_scale() * 1000.).round() as u32
}

fn config() -> Conf {
    Conf {
        window_title: "Rounded panel coverage".into(),
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
        macroquad::logging::error!("ROUNDED_PANELS_FAIL: {info}");
    }));
    PIXELS.store(fixture::run(), Ordering::Relaxed);
    STATUS.store(1, Ordering::Relaxed);
    macroquad::logging::info!("ROUNDED_PANELS_PASS");
    loop {
        clear_background(LIGHTGRAY);
        draw_rectangle_rounded(32., 32., 448., 200., 32., Color::new(0., 0.25, 0.75, 0.5));
        draw_text("Single coverage: passed", 48., 120., 26., WHITE);
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        next_frame().await;
    }
}
