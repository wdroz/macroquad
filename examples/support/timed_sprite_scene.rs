use crate::sheet;
use macroquad::experimental::animation::{
    AnimationPlayback, SpriteClip, SpriteRegionDrawParams, TimedSprite,
};
use macroquad::prelude::*;

pub fn player(playing: bool) -> TimedSprite {
    TimedSprite::new(
        SpriteClip::new(&[
            (sheet::region(0, true), 0.125),
            (sheet::region(1, true), 0.375),
        ])
        .unwrap(),
        playing,
    )
}

pub struct Scene {
    texture: Texture2D,
    pub looping: TimedSprite,
    pub once: TimedSprite,
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
                sprite.seek(0.125).unwrap();
            }
            // Explicit seeking works while paused. A long tick obeys playing.
            let dt = if is_key_pressed(KeyCode::T) {
                2.25
            } else {
                f64::from(get_frame_time())
            };
            sprite.update_with_delta(dt).unwrap();
        }
        set_default_camera();
        clear_background(Color::from_rgba(19, 25, 37, 255));
        draw_text("Per-frame sprite durations", 16., 34., 25., WHITE);
        draw_text("Space: play/pause   R: reset", 16., 61., 18., LIGHTGRAY);
        draw_text(
            "S: seek 0.125s   T: advance 2.25s",
            16.,
            84.,
            18.,
            LIGHTGRAY,
        );
        let size = (screen_width() * 0.27).clamp(60., 150.);
        for (sprite, x, label) in [
            (&self.looping, screen_width() * 0.27, "Loop"),
            (&self.once, screen_width() * 0.73, "Once"),
        ] {
            let anchor = vec2(x, screen_height() * 0.55);
            sprite
                .frame()
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
