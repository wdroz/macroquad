//! Variable-duration clips using the existing checked source-region path.

use super::{AnimationError, AnimationPlayback, SpriteRegion};

/// Why a variable-duration clip could not be constructed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpriteClipError {
    /// At least one region and duration pair is required.
    Empty,
    /// This frame's seconds must be finite and strictly positive.
    InvalidDuration { frame: usize },
    /// Adding this duration overflowed or did not advance the `f64` boundary.
    UnusableTotal { frame: usize },
}

impl std::fmt::Display for SpriteClipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("a sprite clip needs at least one frame"),
            Self::InvalidDuration { frame } => {
                write!(
                    f,
                    "sprite frame {frame} duration must be finite and positive"
                )
            }
            Self::UnusableTotal { frame } => {
                write!(
                    f,
                    "sprite frame {frame} has an unrepresentable end boundary"
                )
            }
        }
    }
}

impl std::error::Error for SpriteClipError {}

/// An immutable sequence of checked regions with individual durations in seconds.
///
/// Regions retain source pixels, trimming and pivots, and own no texture. They
/// may refer to different sheet sizes; the caller supplies the matching texture
/// to [`SpriteRegion::draw`]. Construction rejects empty clips, nonfinite or
/// nonpositive durations, overflowing totals and additions too small to advance
/// the accumulated `f64` boundary. Stored boundaries use ordinary `f64` addition.
/// No frame is duplicated to hold anticipation or recovery poses.
///
/// ```
/// use macroquad::experimental::animation::*;
/// use macroquad::prelude::*;
/// let a = SpriteRegion::from_grid(vec2(24., 8.), vec2(8., 8.), 0, 0).unwrap();
/// let b = SpriteRegion::from_grid(vec2(24., 8.), vec2(8., 8.), 1, 0).unwrap();
/// let clip = SpriteClip::new(&[(a, 0.125), (b, 0.375)]).unwrap();
/// let mut sprite = TimedSprite::new(clip, true);
/// sprite.update_with_delta(0.125).unwrap();
/// assert_eq!(sprite.current_frame(), 1);
/// assert_eq!(sprite.duration(), 0.5);
/// ```
#[derive(Clone, Debug)]
pub struct SpriteClip {
    frames: Vec<(SpriteRegion, f64)>,
    ends: Vec<f64>,
}

impl SpriteClip {
    /// Validates all timings before storing the regions and durations.
    pub fn new(frames: &[(SpriteRegion, f64)]) -> Result<Self, SpriteClipError> {
        if frames.is_empty() {
            return Err(SpriteClipError::Empty);
        }
        let mut total = 0.0;
        let mut ends = Vec::with_capacity(frames.len());
        for (frame, &(_, seconds)) in frames.iter().enumerate() {
            if !seconds.is_finite() || seconds <= 0.0 {
                return Err(SpriteClipError::InvalidDuration { frame });
            }
            let end = total + seconds;
            if !end.is_finite() || end <= total {
                return Err(SpriteClipError::UnusableTotal { frame });
            }
            ends.push(end);
            total = end;
        }
        Ok(Self {
            frames: frames.to_vec(),
            ends,
        })
    }

    /// Region and requested duration pairs in playback order.
    pub fn frames(&self) -> &[(SpriteRegion, f64)] {
        &self.frames
    }

    /// Full accumulated duration, including the last frame.
    pub fn duration(&self) -> f64 {
        self.ends[self.ends.len() - 1]
    }
}

/// Independent, caller-clock playback of a [`SpriteClip`].
///
/// Frame zero starts at zero. Each frame occupies a half-open interval; an exact
/// stored boundary selects the following frame. Looping wraps at the total;
/// once playback holds the last region and finishes only at the total. Seek
/// works while paused and clears completion when seeking before the end.
/// Updates retain fractional seconds and reduce multi-loop deltas without
/// iterating over loops or overflowing their sum. Frame lookup is logarithmic.
/// Floating-point rounding applies; different subdivisions need not be identical.
/// Clones copy their clock and clip without sharing mutable state.
///
/// The player owns no texture and has no automatic frame-time update. Advance
/// it once from the caller's clock, then draw [`Self::frame`] through
/// [`SpriteRegion::draw`]. Pausing, seeking and replaying a one-shot need no GPU:
///
/// ```
/// use macroquad::experimental::animation::{
///     AnimationPlayback, SpriteClip, SpriteRegion, TimedSprite,
/// };
/// use macroquad::math::vec2;
///
/// let first = SpriteRegion::from_grid(vec2(16., 8.), vec2(8., 8.), 0, 0).unwrap();
/// let last = SpriteRegion::from_grid(vec2(16., 8.), vec2(8., 8.), 1, 0).unwrap();
/// let clip = SpriteClip::new(&[(first, 0.125), (last, 0.375)]).unwrap();
/// let mut player = TimedSprite::new(clip, false);
/// player.set_playback(AnimationPlayback::Once);
/// player.seek(0.125).unwrap(); // Seek while paused; exact boundary selects frame 1.
/// player.update_with_delta(10.0).unwrap();
/// assert_eq!(player.elapsed(), 0.125);
/// assert_eq!(player.current_frame(), 1);
/// assert!(!player.is_finished()); // The last frame still has time remaining.
/// player.playing = true;
/// player.update_with_delta(0.375).unwrap();
/// assert!(player.is_finished());
/// assert!(player.playing); // Completion does not change the caller's pause flag.
/// assert!(player.seek(f64::NAN).is_err());
/// assert!(player.is_finished()); // Invalid times leave state unchanged.
/// player.reset(); // Rewind while preserving Once and playing.
/// assert_eq!(player.current_frame(), 0);
/// assert!(!player.is_finished());
/// ```
#[derive(Clone, Debug)]
pub struct TimedSprite {
    clip: SpriteClip,
    elapsed: f64,
    playback: AnimationPlayback,
    /// Pausing preserves time and completion; reset/seek do not resume playback.
    pub playing: bool,
}

impl TimedSprite {
    /// Starts a checked clip at zero in looping mode.
    pub fn new(clip: SpriteClip, playing: bool) -> Self {
        Self {
            clip,
            elapsed: 0.0,
            playback: AnimationPlayback::Loop,
            playing,
        }
    }

    /// Immutable metadata for this player.
    pub fn clip(&self) -> &SpriteClip {
        &self.clip
    }

    /// Current region index; a finished one-shot selects the final region.
    pub fn current_frame(&self) -> usize {
        self.clip
            .ends
            .partition_point(|&end| end <= self.elapsed)
            .min(self.clip.frames.len() - 1)
    }

    /// Checked region ready for the existing anchored drawing path.
    pub fn frame(&self) -> &SpriteRegion {
        &self.clip.frames[self.current_frame()].0
    }

    /// Current wrapped/clamped seconds, including fractional progress.
    pub fn elapsed(&self) -> f64 {
        self.elapsed
    }

    /// Full clip duration in seconds.
    pub fn duration(&self) -> f64 {
        self.clip.duration()
    }

    /// Current end-of-clip policy.
    pub fn playback(&self) -> AnimationPlayback {
        self.playback
    }

    /// True only after once playback reaches the total duration.
    pub fn is_finished(&self) -> bool {
        self.playback == AnimationPlayback::Once && self.elapsed == self.duration()
    }

    /// Changes policy, retaining progress. A completed once player wraps to zero
    /// when changed to looping. Pause state is preserved.
    pub fn set_playback(&mut self, playback: AnimationPlayback) {
        self.playback = playback;
        if playback == AnimationPlayback::Loop && self.elapsed == self.duration() {
            self.elapsed = 0.0;
        }
    }

    /// Rewinds without changing pause state or playback policy.
    pub fn reset(&mut self) {
        self.elapsed = 0.0;
    }

    /// Seeks to finite nonnegative absolute seconds. Looping wraps; once clamps.
    /// Invalid times leave all state unchanged, including while paused/finished.
    pub fn seek(&mut self, seconds: f64) -> Result<(), AnimationError> {
        Self::check_time(seconds)?;
        self.elapsed = match self.playback {
            AnimationPlayback::Loop => seconds % self.duration(),
            AnimationPlayback::Once => seconds.min(self.duration()),
        };
        Ok(())
    }

    /// Advances by finite nonnegative caller-supplied seconds. Paused/finished
    /// players stay still; invalid time still returns an error transactionally.
    pub fn update_with_delta(&mut self, seconds: f64) -> Result<(), AnimationError> {
        Self::check_time(seconds)?;
        if !self.playing || self.is_finished() {
            return Ok(());
        }
        let total = self.duration();
        let remaining = total - self.elapsed;
        self.elapsed = match self.playback {
            AnimationPlayback::Once => {
                if seconds >= remaining {
                    total
                } else {
                    (self.elapsed + seconds).min(total)
                }
            }
            AnimationPlayback::Loop => {
                let delta = seconds % total;
                if delta >= remaining {
                    delta - remaining
                } else {
                    // Rounding may reach the total even when delta < remaining.
                    let time = self.elapsed + delta;
                    if time >= total {
                        0.0
                    } else {
                        time
                    }
                }
            }
        };
        Ok(())
    }

    fn check_time(seconds: f64) -> Result<(), AnimationError> {
        if seconds.is_finite() && seconds >= 0.0 {
            Ok(())
        } else {
            Err(AnimationError::InvalidTime)
        }
    }
}
