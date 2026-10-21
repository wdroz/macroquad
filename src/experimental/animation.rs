//! Checked sprite-sheet metadata and caller-timed animation playback.
//!
//! [`SpriteRegion`] validates explicit rectangles, grid cells and trimmed frames
//! without owning a texture. It places a caller-selected pivot at a fixed drawing
//! anchor, including through flips and rotation. [`SpriteRegionDrawParams`] scales
//! the whole untrimmed frame; [`SpriteRegion::draw`] uses ordinary texture drawing.
//!
//! [`AnimatedSprite`] selects equal-sized cells with one animation per sheet row.
//! [`update_with_delta`](AnimatedSprite::update_with_delta) advances all elapsed
//! frames and retains fractional time. [`update`](AnimatedSprite::update) is a
//! frame-clock convenience. Playback loops by default; [`AnimationPlayback::Once`]
//! holds the last frame after its full duration. Neither playback nor seeking
//! needs a window, texture or GPU.
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

/// What happens after the last frame has been displayed for `1 / fps` seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnimationPlayback {
    /// Return to the first frame, preserving any fractional elapsed time.
    #[default]
    Loop,
    /// Hold the last frame and report [`AnimatedSprite::is_finished`].
    Once,
}

/// Why animation metadata or an elapsed-time operation could not be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationError {
    /// Both tile dimensions must be nonzero.
    ZeroTileSize,
    /// At least one animation must be supplied.
    EmptyAnimations,
    /// The animation at this index must contain at least one frame.
    EmptyClip { animation: usize },
    /// The animation at this index must have a nonzero frame rate.
    ZeroFps { animation: usize },
    /// Elapsed time must be finite and nonnegative.
    InvalidTime,
}

impl std::fmt::Display for AnimationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroTileSize => f.write_str("animation tile dimensions must be nonzero"),
            Self::EmptyAnimations => f.write_str("at least one animation is required"),
            Self::EmptyClip { animation } => {
                write!(f, "animation {animation} must contain at least one frame")
            }
            Self::ZeroFps { animation } => {
                write!(f, "animation {animation} must have a nonzero frame rate")
            }
            Self::InvalidTime => f.write_str("animation time must be finite and nonnegative"),
        }
    }
}

impl std::error::Error for AnimationError {}

/// An independent player for equal-sized sprite-sheet cells, one clip per row.
///
/// Every frame, including the last, lasts `1 / fps` seconds. The first frame is
/// shown at time zero; an exact frame boundary selects the following frame.
/// Time uses `f64` seconds and ordinary floating-point rounding. Replaying the
/// same deltas is deterministic; differently rounded subdivisions need not be
/// bit-identical. Cloning copies the current playback state without sharing it.
///
/// ```
/// use macroquad::experimental::animation::*;
/// let mut sprite = AnimatedSprite::try_new(16, 16, &[
///     Animation { name: "blink".into(), row: 0, frames: 4, fps: 8 },
/// ], true).unwrap();
/// sprite.set_playback(AnimationPlayback::Once);
/// sprite.update_with_delta(0.375).unwrap();
/// assert!(sprite.is_last_frame());
/// assert!(!sprite.is_finished()); // The final frame still has its own duration.
/// sprite.update_with_delta(0.125).unwrap();
/// assert!(sprite.is_finished());
/// sprite.reset();
/// assert_eq!(sprite.current_frame(), 0);
/// ```
#[derive(Clone)]
pub struct AnimatedSprite {
    tile_width: f32,
    tile_height: f32,
    animations: Vec<Animation>,

    current_animation: usize,
    frame_time: f64,
    frame: u32,
    playback: AnimationPlayback,
    finished: bool,
    /// Whether elapsed-time updates advance playback. Pausing retains the
    /// current frame and fractional time. Finishing a one-shot does not change
    /// this flag; reset or seek to an earlier time to play it again.
    pub playing: bool,
}

impl AnimatedSprite {
    /// Stores equal-sized tile dimensions and row-based animation descriptions.
    ///
    /// Starts on frame zero of the first animation in looping mode.
    ///
    /// # Panics
    /// Panics for zero tile dimensions, no animations, or any zero frame count
    /// or FPS. Use [`Self::try_new`] to handle these errors explicitly. Neither
    /// constructor knows the texture dimensions; validate frames with
    /// [`SpriteRegion`] before drawing.
    pub fn new(
        tile_width: u32,
        tile_height: u32,
        animations: &[Animation],
        playing: bool,
    ) -> AnimatedSprite {
        Self::try_new(tile_width, tile_height, animations, playing)
            .expect("invalid sprite animation metadata")
    }

    /// Checks tile dimensions and every clip before creating a looping player.
    ///
    /// Row indices and frame counts are source-sheet metadata, not texture
    /// bounds. Validate the returned [`AnimationFrame::source_rect`] with
    /// [`SpriteRegion::new`] and the actual sheet size before drawing it.
    /// Existing [`Animation`] and [`AnimationFrame`] struct literals are unchanged.
    ///
    /// ```
    /// use macroquad::experimental::animation::*;
    /// assert!(matches!(AnimatedSprite::try_new(16, 16, &[], true),
    ///     Err(AnimationError::EmptyAnimations)));
    /// ```
    pub fn try_new(
        tile_width: u32,
        tile_height: u32,
        animations: &[Animation],
        playing: bool,
    ) -> Result<AnimatedSprite, AnimationError> {
        if tile_width == 0 || tile_height == 0 {
            return Err(AnimationError::ZeroTileSize);
        }
        if animations.is_empty() {
            return Err(AnimationError::EmptyAnimations);
        }
        for (index, animation) in animations.iter().enumerate() {
            if animation.frames == 0 {
                return Err(AnimationError::EmptyClip { animation: index });
            }
            if animation.fps == 0 {
                return Err(AnimationError::ZeroFps { animation: index });
            }
        }
        Ok(AnimatedSprite {
            tile_width: tile_width as f32,
            tile_height: tile_height as f32,
            animations: animations.to_vec(),
            current_animation: 0,
            frame_time: 0.0,
            frame: 0,
            playback: AnimationPlayback::Loop,
            finished: false,
            playing,
        })
    }

    /// Chooses an animation by index, retaining the accumulated time.
    ///
    /// The frame index is reduced modulo the new animation's frame count, the
    /// fractional elapsed seconds are retained, and completion is cleared.
    /// The next elapsed-time update consumes any remainder exceeding the new
    /// frame duration. Pause and playback mode are unchanged. Call [`Self::reset`]
    /// afterward to start the selected clip at time zero.
    ///
    /// # Panics
    /// The index must exist.
    pub fn set_animation(&mut self, animation: usize) {
        let seconds = self.frame_time / f64::from(self.animations[self.current_animation].fps);
        self.current_animation = animation;

        let animation = &self.animations[self.current_animation];
        self.frame %= animation.frames;
        self.frame_time = seconds * f64::from(animation.fps);
        self.finished = false;
    }

    /// Currently chosen animation
    pub const fn current_animation(&self) -> usize {
        self.current_animation
    }

    /// The current zero-based frame index within the selected animation.
    pub const fn current_frame(&self) -> u32 {
        self.frame
    }

    /// Sets a frame index without resetting the accumulated time.
    ///
    /// Supply an index below the current animation's frame count. This method
    /// does not validate it; [`Self::frame`] reports the corresponding rectangle.
    /// Completion is cleared; pause and playback mode are unchanged. Use
    /// [`Self::seek`] for a checked absolute time or [`Self::reset`] to rewind.
    pub fn set_frame(&mut self, frame: u32) {
        self.frame = frame;
        self.finished = false;
    }

    /// Returns whether the last frame is being displayed, not whether playback completed.
    ///
    /// Use [`Self::is_finished`] for one-shot completion, which happens only
    /// after the last frame has been displayed for its full duration.
    pub fn is_last_frame(&self) -> bool {
        let animation = &self.animations[self.current_animation];
        self.frame == animation.frames - 1
    }

    /// Advances using [`get_frame_time`], once per application frame.
    ///
    /// Delegates to [`Self::update_with_delta`], advancing all elapsed frames and
    /// retaining fractional time. An invalid global delta is ignored. This
    /// convenience needs an initialized macroquad context; caller-timed updates
    /// do not.
    pub fn update(&mut self) {
        let _ = self.update_with_delta(f64::from(get_frame_time()));
    }

    /// Advances by finite nonnegative elapsed seconds supplied by the caller.
    ///
    /// Multiple frames/cycles are consumed in constant time, without dropping
    /// the fractional remainder. Negative or nonfinite deltas return an error
    /// without changing any state, including while paused or finished. A paused
    /// or finished player otherwise stays still. Zero seconds can normalize a
    /// retained remainder after [`Self::set_animation`].
    ///
    /// ```
    /// use macroquad::experimental::animation::*;
    /// let mut sprite = AnimatedSprite::new(8, 8, &[
    ///     Animation { name: "walk".into(), row: 0, frames: 4, fps: 8 },
    /// ], true);
    /// sprite.update_with_delta(0.3125).unwrap(); // 2.5 frames
    /// assert_eq!(sprite.current_frame(), 2);
    /// sprite.playing = false;
    /// sprite.update_with_delta(10.0).unwrap();
    /// sprite.playing = true;
    /// sprite.update_with_delta(0.0625).unwrap();
    /// assert_eq!(sprite.current_frame(), 3);
    /// ```
    pub fn update_with_delta(&mut self, seconds: f64) -> Result<(), AnimationError> {
        Self::check_time(seconds)?;
        if !self.playing || self.finished {
            return Ok(());
        }
        let animation = &self.animations[self.current_animation];
        self.frame %= animation.frames;
        if self.playback == AnimationPlayback::Once {
            let remaining = (f64::from(animation.frames - self.frame) - self.frame_time)
                / f64::from(animation.fps);
            if seconds >= remaining {
                self.finish();
                return Ok(());
            }
        }
        let (whole, fractional) = Self::elapsed_frames(seconds, animation);
        let progress = self.frame_time + fractional;
        let frames = u64::from(self.frame) + whole + progress.floor() as u64;
        self.frame_time = progress.fract();
        if self.playback == AnimationPlayback::Once && frames >= u64::from(animation.frames) {
            self.finish();
        } else {
            self.frame = (frames % u64::from(animation.frames)) as u32;
        }
        Ok(())
    }

    /// Rewinds to the first frame and clears fractional time and completion.
    /// The selected animation, pause state and playback mode are preserved.
    pub fn reset(&mut self) {
        self.frame = 0;
        self.frame_time = 0.;
        self.finished = false;
    }

    /// Seeks to finite nonnegative absolute seconds within the current clip.
    /// Looping wraps; one-shot playback clamps at completion. Seeking before
    /// completion clears that flag, and works even while paused. Errors leave
    /// state unchanged. Pause state and playback mode are preserved.
    ///
    /// ```
    /// use macroquad::experimental::animation::*;
    /// let mut sprite = AnimatedSprite::new(8, 8, &[
    ///     Animation { name: "walk".into(), row: 0, frames: 4, fps: 8 },
    /// ], false);
    /// sprite.seek(0.25).unwrap();
    /// assert_eq!(sprite.current_frame(), 2);
    /// assert!(!sprite.playing);
    /// assert!(sprite.seek(f64::NAN).is_err());
    /// ```
    pub fn seek(&mut self, seconds: f64) -> Result<(), AnimationError> {
        Self::check_time(seconds)?;
        if self.playback == AnimationPlayback::Once && seconds >= self.duration() {
            self.finish();
        } else {
            let animation = &self.animations[self.current_animation];
            let (whole, fractional) = Self::elapsed_frames(seconds, animation);
            self.frame = (whole % u64::from(animation.frames)) as u32;
            self.frame_time = fractional;
            self.finished = false;
        }
        Ok(())
    }

    /// Selects looping or one-shot playback without restarting an in-progress
    /// clip. Switching a completed one-shot to looping wraps to frame zero.
    /// Pause state is preserved. Use [`Self::reset`] to explicitly restart.
    pub fn set_playback(&mut self, playback: AnimationPlayback) {
        self.playback = playback;
        if self.finished && playback == AnimationPlayback::Loop {
            self.reset();
        }
    }

    /// The current loop/one-shot policy.
    pub const fn playback(&self) -> AnimationPlayback {
        self.playback
    }

    /// Whether one-shot playback reached the end of the final frame's duration.
    /// This is independent of [`Self::playing`]; looping playback never finishes.
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    /// Current clip seconds after wrapping/clamping, including the fractional
    /// frame time. A completed one-shot reports [`Self::duration`]. Legacy
    /// frame/animation setters can leave an unnormalized remainder until the
    /// next playing update; [`Self::seek`] always normalizes immediately.
    pub fn elapsed(&self) -> f64 {
        if self.finished {
            self.duration()
        } else {
            f64::from(self.frame) / f64::from(self.animations[self.current_animation].fps)
                + self.frame_time / f64::from(self.animations[self.current_animation].fps)
        }
    }

    /// The full current clip duration, including the last frame, in seconds.
    pub fn duration(&self) -> f64 {
        let animation = &self.animations[self.current_animation];
        f64::from(animation.frames) / f64::from(animation.fps)
    }

    fn check_time(seconds: f64) -> Result<(), AnimationError> {
        if seconds.is_finite() && seconds >= 0. {
            Ok(())
        } else {
            Err(AnimationError::InvalidTime)
        }
    }

    // For ordinary deltas, multiplication rounds once at frame boundaries.
    // Beyond f64's exact-integer range, reduce whole seconds before multiplying
    // by FPS: even f64::MAX is safe, and the u32 metadata product fits in u64.
    // Keep fractions separate from the current frame index.
    fn elapsed_frames(seconds: f64, animation: &Animation) -> (u64, f64) {
        let count = u64::from(animation.frames);
        if seconds <= (1_u64 << 53) as f64 / f64::from(animation.fps) {
            let fps = f64::from(animation.fps);
            let mut whole = (seconds * fps).floor();
            // Multiplication can round across a division-defined boundary.
            // Compare seconds to that boundary, rather than applying an epsilon
            // that would also advance its representable predecessor.
            if seconds < whole / fps {
                whole -= 1.;
            } else if seconds >= (whole + 1.) / fps {
                whole += 1.;
            }
            let fractional =
                ((seconds - whole / fps) * fps).clamp(0., f64::from_bits(1_f64.to_bits() - 1));
            return ((whole as u64) % count, fractional);
        }
        let seconds_mod = (seconds.trunc() % f64::from(animation.frames)) as u64;
        let whole = (seconds_mod * u64::from(animation.fps)) % count;
        let fractional = seconds.fract() * f64::from(animation.fps);
        (whole + fractional.floor() as u64, fractional.fract())
    }

    fn finish(&mut self) {
        self.frame = self.animations[self.current_animation].frames - 1;
        self.frame_time = 0.;
        self.finished = true;
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
