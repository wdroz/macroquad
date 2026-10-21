//! Shared native/WebGL timing and pixel checks, then a live playback demo.
#[path = "../tests/support/sprite_animation_fixture.rs"]
mod fixture;
#[path = "support/sprite_animation_scene.rs"]
mod scene;
#[path = "support/sprite_regions_sheet.rs"]
#[allow(dead_code)]
mod sheet;
use macroquad::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
static STATUS: AtomicU32 = AtomicU32::new(0);
static CHECKS: AtomicU32 = AtomicU32::new(0);
static STATE: AtomicU32 = AtomicU32::new(0);
static FRAMES: AtomicU32 = AtomicU32::new(0);
#[no_mangle]
pub extern "C" fn sprite_animation_status() -> u32 {
    STATUS.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn sprite_animation_checks() -> u32 {
    CHECKS.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn sprite_animation_state() -> u32 {
    STATE.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn sprite_animation_frames() -> u32 {
    FRAMES.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn sprite_animation_dpi_milli() -> u32 {
    (screen_dpi_scale() * 1000.).round() as u32
}
fn config() -> Conf {
    Conf {
        window_title: "Sprite animation checks".into(),
        window_width: 390,
        window_height: 400,
        high_dpi: true,
        ..Default::default()
    }
}
#[macroquad::main(config)]
async fn main() {
    std::panic::set_hook(Box::new(|info| {
        STATUS.store(2, Ordering::Relaxed);
        macroquad::logging::error!("SPRITE_ANIMATION_FAIL: {}", info);
    }));
    CHECKS.store(fixture::run().await, Ordering::Relaxed);
    STATUS.store(1, Ordering::Relaxed);
    macroquad::logging::info!("SPRITE_ANIMATION_PASS");
    let mut scene = scene::Scene::new();
    loop {
        scene.draw();
        STATE.store(
            scene.looping.current_frame()
                | scene.once.current_frame() << 1
                | u32::from(scene.once.is_finished()) << 2
                | u32::from(scene.looping.playing) << 3,
            Ordering::Relaxed,
        );
        FRAMES.fetch_add(1, Ordering::Relaxed);
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        next_frame().await;
    }
}
