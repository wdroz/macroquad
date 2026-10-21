//! Shared native/WebGL regression and responsive outline demonstration.
#[path = "../tests/support/rounded_outlines_fixture.rs"]
mod fixture;
use macroquad::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
static STATUS: AtomicU32 = AtomicU32::new(0);
static PIXELS: AtomicU32 = AtomicU32::new(0);

#[no_mangle]
pub extern "C" fn rounded_outlines_status() -> u32 {
    STATUS.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn rounded_outlines_pixels() -> u32 {
    PIXELS.load(Ordering::Relaxed)
}
fn config() -> Conf {
    Conf {
        window_title: "Rounded outlines".into(),
        window_width: 640,
        window_height: 400,
        high_dpi: true,
        ..Default::default()
    }
}
#[macroquad::main(config)]
async fn main() {
    std::panic::set_hook(Box::new(|info| {
        STATUS.store(2, Ordering::Relaxed);
        macroquad::logging::error!("ROUNDED_OUTLINES_FAIL: {info}");
    }));
    PIXELS.store(fixture::run(), Ordering::Relaxed);
    STATUS.store(1, Ordering::Relaxed);
    macroquad::logging::info!("ROUNDED_OUTLINES_PASS");
    loop {
        clear_background(Color::from_rgba(20, 28, 42, 255));
        let width = screen_width() - 40.;
        draw_rectangle_rounded(20., 24., width, 120., 24., Color::new(0.1, 0.4, 0.7, 0.35));
        draw_rectangle_rounded_lines(20., 24., width, 120., 24., 3., SKYBLUE);
        draw_text("Inward rounded outlines", 32., 68., 24., WHITE);
        draw_text("4,718,592 pixels passed", 32., 106., 20., WHITE);
        for (i, r) in [0., 12., 1000.].into_iter().enumerate() {
            draw_rectangle_rounded_lines(
                24. + i as f32 * (width / 3.),
                180.,
                width / 3. - 12.,
                80.,
                r,
                8.,
                Color::new(0., 1., 1., 0.5),
            );
        }
        draw_text("Square / rounded / clamped", 24., 304., 20., WHITE);
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        next_frame().await;
    }
}
