//! Space plays/pauses; R resets; S seeks; T supplies a long elapsed-time tick.
#[path = "support/sprite_animation_scene.rs"]
mod scene;
#[path = "support/sprite_regions_sheet.rs"]
#[allow(dead_code)]
mod sheet;
use macroquad::prelude::*;

#[macroquad::main("Sprite animation timing")]
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
