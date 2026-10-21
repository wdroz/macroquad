//! Click the animated sprite, scroll the log, and pause with Escape or the HUD.
#[path = "support/sprite_ui_scene.rs"]
mod scene;
#[path = "support/sprite_regions_sheet.rs"]
#[allow(dead_code)]
mod sheet;
use macroquad::prelude::*;
use scene::config;

#[macroquad::main(config)]
async fn main() {
    let mut scene = scene::Scene::new();
    loop {
        scene.frame(f64::from(get_frame_time()));
        next_frame().await;
    }
}
