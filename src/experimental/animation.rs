//! Sprite-sheet metadata and the existing frame-clock animation player.
//!
//! [`SpriteRegion`] validates explicit rectangles, grid cells and trimmed frames
//! without owning a texture. It places a caller-selected pivot at a fixed drawing
//! anchor, including through flips and rotation. [`SpriteRegionDrawParams`] scales
//! the whole untrimmed frame; [`SpriteRegion::draw`] uses ordinary texture drawing.
//!
//! [`AnimatedSprite`] selects equal-sized cells with one animation per sheet row.
//! Its existing [`update`](AnimatedSprite::update) reads the global frame clock,
//! loops and advances at most once per call. Region validation does not change
//! that timing or add a one-shot/elapsed-time player.
//!
//! # Examples
//! Let's say we have an image of our character where every frame is 15x20 rect and we have this animations:
//! - Idle animation with 20 frames at 12 fps
//! - Run animation with 15 frames at 15 fps
//!
//! ```no_run
//! use macroquad::experimental::animation::*;
//! use macroquad::prelude::*;
//!
//! #[macroquad::main("Animation")]
//! async fn main() {
//!     // Define animations
//!     let mut sprite = AnimatedSprite::new(
//!         15,
//!         20,
//!         &[
//!             Animation {
//!                 name: "idle".to_string(),
//!                 row: 0,
//!                 frames: 20,
//!                 fps: 12,
//!             },
//!             Animation {
//!                 name: "run".to_string(),
//!                 row: 1,
//!                 frames: 15,
//!                 fps: 15,
//!             },
//!         ],
//!         true,
//!     );
//!     let image = load_texture("some_path.png").await.unwrap();
//!     loop {
//!         clear_background(WHITE);
//!         // Now we can draw our character
//!         draw_texture_ex(
//!             &image,
//!             10.,
//!             10.,
//!             WHITE,
//!             DrawTextureParams {
//!                 source: Some(sprite.frame().source_rect),
//!                 dest_size: Some(sprite.frame().dest_size),
//!                 ..Default::default()
//!             }
//!         );
//!         // Update frame
//!         sprite.update();
//!         next_frame().await;
//!     }
//! }
//! ```

use crate::{
    math::{vec2, Rect, Vec2},
    time::get_frame_time,
};

mod sprite_region;
pub use sprite_region::{SpriteRegion, SpriteRegionDrawParams, SpriteRegionError};

/// Specification of animation
#[derive(Clone, Debug)]
pub struct Animation {
    pub name: String,
    pub row: u32,
    pub frames: u32,
    pub fps: u32,
}

/// Specific animation frame
pub struct AnimationFrame {
    /// Area of current frame in source image
    pub source_rect: Rect,
    /// Size of frame
    pub dest_size: Vec2,
}

/// Main definition of all animations for specific image
#[derive(Clone)]
pub struct AnimatedSprite {
    tile_width: f32,
    tile_height: f32,
    animations: Vec<Animation>,

    current_animation: usize,
    time: f32,
    frame: u32,
    /// Controls if frame should be updated on [update][Self::update]
    pub playing: bool,
}

impl AnimatedSprite {
    /// Stores equal-sized tile dimensions and row-based animation descriptions.
    ///
    /// This constructor does not validate them: supply positive tile dimensions,
    /// a nonempty animation slice, positive frame counts/FPS and a sufficiently
    /// large texture. Use [`SpriteRegion`] to validate a returned source rectangle
    /// against the texture dimensions before drawing it.
    pub fn new(
        tile_width: u32,
        tile_height: u32,
        animations: &[Animation],
        playing: bool,
    ) -> AnimatedSprite {
        AnimatedSprite {
            tile_width: tile_width as f32,
            tile_height: tile_height as f32,
            animations: animations.to_vec(),
            current_animation: 0,
            time: 0.0,
            frame: 0,
            playing,
        }
    }

    /// Chooses an animation by index, retaining the accumulated time.
    ///
    /// The frame index is reduced modulo the new animation's frame count.
    /// [`Self::set_frame`] can select its first frame, but does not reset time.
    /// The index must exist and its frame count must be positive.
    pub fn set_animation(&mut self, animation: usize) {
        self.current_animation = animation;

        let animation = &self.animations[self.current_animation];
        self.frame %= animation.frames;
    }

    /// Currently chosen animation
    pub const fn current_animation(&self) -> usize {
        self.current_animation
    }

    /// Sets a frame index without resetting the accumulated time.
    ///
    /// Supply an index below the current animation's frame count. This method
    /// does not validate it; [`Self::frame`] reports the corresponding rectangle.
    pub fn set_frame(&mut self, frame: u32) {
        self.frame = frame;
    }

    /// Returns whether the last frame is being displayed, not whether playback completed.
    ///
    /// Playback loops; this is not a one-shot completion flag.
    pub fn is_last_frame(&self) -> bool {
        let animation = &self.animations[self.current_animation];
        self.frame == animation.frames - 1
    }

    /// Advances the looping player using [`get_frame_time`].
    ///
    /// While [`Self::playing`] is true, time accumulates until it is strictly
    /// greater than `1. / fps`. This call advances at most one frame and discards
    /// the remainder when advancing, even after a long tick. Pausing retains
    /// accumulated time. The frame index is reduced modulo the clip length on
    /// every call, including while paused. This is not deterministic elapsed-time
    /// playback; call it once per application frame with valid animation data.
    pub fn update(&mut self) {
        let animation = &self.animations[self.current_animation];

        if self.playing {
            self.time += get_frame_time();
            if self.time > 1. / animation.fps as f32 {
                self.frame += 1;
                self.time = 0.0;
            }
        }
        self.frame %= animation.frames;
    }

    /// Get current frame
    pub fn frame(&self) -> AnimationFrame {
        let animation = &self.animations[self.current_animation];

        AnimationFrame {
            source_rect: Rect::new(
                self.tile_width * self.frame as f32,
                self.tile_height * animation.row as f32,
                self.tile_width,
                self.tile_height,
            ),
            dest_size: vec2(self.tile_width, self.tile_height),
        }
    }
}
