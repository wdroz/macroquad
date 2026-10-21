//! Resize and click a sprite drawn into a differently sized offscreen target.
//! The shared scene contains the complete public camera/display conversion path.
#[path = "support/camera_target_scene.rs"]
mod scene;
use macroquad::prelude::*;

fn config() -> Conf {
    Conf {
        window_title: "Camera2D target picking".into(),
        window_width: 960,
        window_height: 640,
        high_dpi: true,
        ..Default::default()
    }
}

#[macroquad::main(config)]
async fn main() {
    let mut scene = scene::Scene::new();
    let mut previous_hits = 0;
    loop {
        scene.frame();
        if scene.hits != previous_hits {
            macroquad::logging::info!("Picked sprite {}", scene.hits);
            previous_hits = scene.hits;
        }
        // Also exercise world -> target -> display for a small HUD marker.
        let position = scene.sprite_window_position();
        draw_circle_lines(position.x, position.y, 4., 1., WHITE);
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        next_frame().await;
    }
}
