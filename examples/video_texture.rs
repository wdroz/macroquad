//! cargo run --features video --example video_texture -- path/to/clip.mp4
#[cfg(feature = "video")]
use macroquad::prelude::*;

#[cfg(feature = "video")]
#[macroquad::main("Video texture — Space pause, M mute, R restart")]
async fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "clip.mp4".into());
    let mut player = match VideoPlayer::load(&path, VideoOptions::default()).await {
        Ok(player) => player,
        Err(error) => {
            eprintln!("{error}");
            return;
        }
    };
    let mut muted = false;
    let mut error = String::new();
    // Press Space to begin so browsers receive an explicit play interaction.
    loop {
        if is_key_pressed(KeyCode::Space) {
            if player.state() == VideoState::Playing {
                player.pause();
            } else if let Err(e) = player.play() {
                error = e.to_string();
            }
        }
        if is_key_pressed(KeyCode::M) {
            muted = !muted;
            player.set_muted(muted);
        }
        if is_key_pressed(KeyCode::R) {
            player.stop();
        }
        if let Err(e) = player.update() {
            error = e.to_string();
        }
        clear_background(BLACK);
        if let Some(texture) = player.texture() {
            let scale =
                (screen_width() / texture.width()).min((screen_height() - 80.0) / texture.height());
            let size = texture.size() * scale;
            draw_texture_ex(
                texture,
                (screen_width() - size.x) * 0.5,
                0.0,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(size),
                    ..Default::default()
                },
            );
        }
        draw_text(
            &format!(
                "Space play/pause · M mute · R stop  {:.1}s {:?}",
                player.position(),
                player.state()
            ),
            15.0,
            screen_height() - 45.0,
            20.0,
            WHITE,
        );
        draw_text(&error, 15.0, screen_height() - 15.0, 18.0, RED);
        next_frame().await;
    }
}
#[cfg(not(feature = "video"))]
fn main() {
    eprintln!("Enable the video feature to run this example");
}
