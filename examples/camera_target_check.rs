//! Shared native/WebGL pixel checks, followed by the actual interactive demo.
#[path = "../tests/support/camera_target_fixture.rs"]
mod fixture;
#[path = "support/camera_target_scene.rs"]
mod scene;
use macroquad::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
static STATUS: AtomicU32 = AtomicU32::new(0);
static CHECKS: AtomicU32 = AtomicU32::new(0);
static HITS: AtomicU32 = AtomicU32::new(0);
static MISSES: AtomicU32 = AtomicU32::new(0);
static FRAMES: AtomicU32 = AtomicU32::new(0);
static POSITION: [AtomicU32; 2] = [const { AtomicU32::new(0) }; 2];
#[no_mangle]
pub extern "C" fn camera_target_status() -> u32 {
    STATUS.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn camera_target_checks() -> u32 {
    CHECKS.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn camera_target_hits() -> u32 {
    HITS.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn camera_target_misses() -> u32 {
    MISSES.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn camera_target_frames() -> u32 {
    FRAMES.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn camera_target_sprite(component: u32) -> f32 {
    POSITION
        .get(component as usize)
        .map_or(0., |p| f32::from_bits(p.load(Ordering::Relaxed)))
}
#[no_mangle]
pub extern "C" fn camera_target_dpi_milli() -> u32 {
    (screen_dpi_scale() * 1000.).round() as u32
}
fn config() -> Conf {
    Conf {
        window_title: "Camera2D target checks".into(),
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
        macroquad::logging::error!("CAMERA_TARGET_FAIL: {}", info);
    }));
    CHECKS.store(fixture::run().await, Ordering::Relaxed);
    STATUS.store(1, Ordering::Relaxed);
    macroquad::logging::info!("CAMERA_TARGET_PASS");
    let mut scene = scene::Scene::new();
    loop {
        scene.frame();
        let point = scene.sprite_window_position();
        POSITION[0].store(point.x.to_bits(), Ordering::Relaxed);
        POSITION[1].store(point.y.to_bits(), Ordering::Relaxed);
        HITS.store(scene.hits, Ordering::Relaxed);
        MISSES.store(scene.misses, Ordering::Relaxed);
        FRAMES.fetch_add(1, Ordering::Relaxed);
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        next_frame().await;
    }
}
