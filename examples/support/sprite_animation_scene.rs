use crate::sheet;
use macroquad::experimental::animation::{
    AnimatedSprite, Animation, AnimationPlayback, SpriteRegion, SpriteRegionDrawParams,
};
use macroquad::prelude::*;

pub fn player(playing: bool) -> AnimatedSprite {
    AnimatedSprite::try_new(
        20,
        20,
        &[Animation {
            name: "pulse".into(),
            row: 0,
            frames: 2,
            fps: 2,
        }],
        playing,
    )
    .unwrap()
}

pub struct Scene {
    texture: Texture2D,
    pub looping: AnimatedSprite,
    pub once: AnimatedSprite,
}

impl Scene {
    pub fn new() -> Self {
        let mut once = player(false);
        once.set_playback(AnimationPlayback::Once);
        Self {
            texture: sheet::texture(),
            looping: player(false),
            once,
        }
    }

    pub fn draw(&mut self) {
        if is_key_pressed(KeyCode::Space) {
            self.looping.playing = !self.looping.playing;
            self.once.playing = self.looping.playing;
        }
        for sprite in [&mut self.looping, &mut self.once] {
            if is_key_pressed(KeyCode::R) {
                sprite.reset();
            }
            if is_key_pressed(KeyCode::S) {
                sprite.seek(0.5).unwrap();
            }
            // Explicit seeking works while paused. A long tick obeys playing.
            let dt = if is_key_pressed(KeyCode::T) {
                2.75
            } else {
                f64::from(get_frame_time())
            };
            sprite.update_with_delta(dt).unwrap();
        }
        set_default_camera();
        clear_background(Color::from_rgba(19, 25, 37, 255));
        draw_text("Deterministic sprite timing", 16., 34., 25., WHITE);
        draw_text("Space: play/pause   R: reset", 16., 61., 18., LIGHTGRAY);
        draw_text("S: seek 0.5s   T: advance 2.75s", 16., 84., 18., LIGHTGRAY);
        let size = (screen_width() * 0.27).clamp(60., 150.);
        for (sprite, x, label) in [
            (&self.looping, screen_width() * 0.27, "Loop"),
            (&self.once, screen_width() * 0.73, "Once"),
        ] {
            let anchor = vec2(x, screen_height() * 0.55);
            SpriteRegion::new(sheet::SHEET_SIZE, sprite.frame().source_rect)
                .unwrap()
                .with_pivot(sheet::PIVOT)
                .unwrap()
                .draw(
                    &self.texture,
                    anchor,
                    WHITE,
                    SpriteRegionDrawParams {
                        dest_size: Some(vec2(size, size)),
                        ..Default::default()
                    },
                )
                .unwrap();
            draw_text(label, x - 28., anchor.y - size * 0.65, 22., WHITE);
            draw_text(
                format!("Frame {}", sprite.current_frame()),
                x - 32.,
                anchor.y + size * 0.75,
                18.,
                WHITE,
            );
            if sprite.is_finished() {
                draw_text(
                    "Finished",
                    x - 34.,
                    anchor.y + size * 0.75 + 24.,
                    18.,
                    GREEN,
                );
            }
        }
        draw_text(
            if self.looping.playing {
                "Playing"
            } else {
                "Paused (fraction retained)"
            },
            16.,
            screen_height() - 22.,
            18.,
            WHITE,
        );
    }
}
