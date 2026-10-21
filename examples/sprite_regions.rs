//! Press Space to change trimmed frame; X/Y mirror about its stable pivot.
#[path = "support/sprite_regions_scene.rs"]
mod scene;
#[path = "support/sprite_regions_sheet.rs"]
mod sheet;
use macroquad::prelude::*;
#[macroquad::main("Checked sprite regions")]
async fn main() {
    let mut scene = scene::Scene::new();
    loop {
        scene.draw();
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        next_frame().await;
    }
}
