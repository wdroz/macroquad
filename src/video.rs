//! Bounded video playback into a regular [`Texture2D`], including synchronized audio.
//!
//! Enable `video`. Native builds need FFmpeg and FFprobe on PATH (or the
//! `MACROQUAD_FFMPEG` / `MACROQUAD_FFPROBE` environment variables). Web builds
//! need `js/mq_video.js` after `mq_js_bundle.js`, before `load(...)`.
//! See `docs/video.md` for platform requirements and limitations.

use crate::texture::{FilterMode, Texture2D};

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(not(target_arch = "wasm32"))]
use native::Backend;
#[cfg(target_arch = "wasm32")]
use web::Backend;

/// Readable metadata, decoder, audio-device, or browser playback failure.
///
/// ```
/// use macroquad::video::VideoError;
/// let error = VideoError("No supported audio device".into());
/// assert_eq!(error.to_string(), "No supported audio device");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoError(pub String);
impl std::fmt::Display for VideoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for VideoError {}

/// Texture bounds and playback settings, validated by [`VideoPlayer::load`].
///
/// Both bounds must be in `1..=4096`, and `fps` in `1..=120`. Volume must
/// be finite and is clamped to `0.0..=1.0`. Defaults are 1280 x 720, 24 fps,
/// no loop, and full volume. The browser keeps the source presentation rate.
///
/// ```
/// use macroquad::video::VideoOptions;
/// let portrait = VideoOptions {
///     max_width: 720,
///     max_height: 1280,
///     fps: 25,
///     ..Default::default()
/// };
/// assert!(!portrait.looped);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct VideoOptions {
    /// Decoded texture bounds; aspect ratio is preserved without upscaling.
    pub max_width: u16,
    pub max_height: u16,
    /// Native decode rate. The browser uses the source's presentation rate.
    pub fps: u16,
    /// Restart at the end; native looping restarts decoding and is not seamless.
    pub looped: bool,
    /// Initial soundtrack volume, clamped to `0.0..=1.0` when finite.
    pub volume: f32,
}
impl Default for VideoOptions {
    fn default() -> Self {
        Self {
            max_width: 1280,
            max_height: 720,
            fps: 24,
            looped: false,
            volume: 1.0,
        }
    }
}
impl VideoOptions {
    fn validate(self) -> Result<Self, VideoError> {
        if self.max_width == 0
            || self.max_height == 0
            || self.max_width > 4096
            || self.max_height > 4096
            || self.fps == 0
            || self.fps > 120
            || !self.volume.is_finite()
        {
            return Err(VideoError(
                "Video bounds must be 1..4096, fps 1..120, and volume finite".into(),
            ));
        }
        Ok(Self {
            volume: self.volume.clamp(0.0, 1.0),
            ..self
        })
    }
}

/// Playback state reported by [`VideoPlayer::state`].
///
/// ```no_run
/// use macroquad::video::{VideoPlayer, VideoState};
/// # fn replay(video: &mut VideoPlayer) -> Result<(), macroquad::video::VideoError> {
/// if video.state() == VideoState::Ended {
///     video.play()?;
/// }
/// # Ok(()) }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoState {
    /// Loaded or stopped, with no frame texture allocated.
    Ready,
    /// Playback requested; call [`VideoPlayer::update`] to advance frames.
    Playing,
    /// Playback suspended, preserving its position.
    Paused,
    /// The clip reached its end; [`VideoPlayer::play`] restarts it.
    Ended,
}

/// Owns decoder/audio resources. Dropping or stopping releases native decoder
/// processes; dropping also releases browser media and this player's texture
/// handle. Texture clones and queued draws can keep that GPU texture alive.
/// Call `update` before drawing each frame. The returned texture can be used by
/// any normal macroquad 2D/3D mesh, including an emissive screen material.
///
/// Native audio currently requires Windows; silent native video and browser
/// playback have the platform support described in `docs/video.md`.
///
/// ```no_run
/// use macroquad::prelude::*;
/// use macroquad::video::{VideoOptions, VideoPlayer};
/// #[macroquad::main("Video texture")]
/// async fn main() {
///     let mut video = VideoPlayer::load("assets/clip.mp4", VideoOptions::default())
///         .await.expect("video metadata");
///     loop {
///         if is_key_pressed(KeyCode::Space) {
///             video.play().expect("start playback");
///         }
///         video.update().expect("advance playback");
///         clear_background(BLACK);
///         if let Some(texture) = video.texture() {
///             draw_texture(texture, 0.0, 0.0, WHITE);
///         }
///         next_frame().await;
///     }
/// }
/// ```
pub struct VideoPlayer {
    backend: Backend,
    texture: Option<Texture2D>,
}
impl VideoPlayer {
    /// Load metadata without starting playback or allocating a frame texture.
    ///
    /// Call on the macroquad thread. Native paths must name local files and
    /// need FFmpeg/FFprobe. Browser paths follow URL/CORS rules and require
    /// `mq_video.js`. Invalid options or metadata failures return an error.
    /// Dropping an unfinished load cancels its metadata work.
    ///
    /// ```no_run
    /// use macroquad::video::{VideoError, VideoOptions, VideoPlayer};
    /// # async fn load() -> Result<(), VideoError> {
    /// let video = VideoPlayer::load("assets/clip.mp4", VideoOptions::default()).await?;
    /// assert!(video.texture().is_none());
    /// # Ok(()) }
    /// ```
    pub async fn load(path: &str, options: VideoOptions) -> Result<Self, VideoError> {
        let options = options.validate()?;
        let backend = Backend::load(path, options).await?;
        Ok(Self {
            backend,
            texture: None,
        })
    }
    /// Start, resume, or restart an ended clip. Repeated calls while playing
    /// have no effect. Browser autoplay failures can arrive later in [`Self::update`].
    ///
    /// ```no_run
    /// # use macroquad::video::{VideoError, VideoPlayer};
    /// # fn start(video: &mut VideoPlayer) -> Result<(), VideoError> {
    /// video.play()?; // typically after an explicit Play input
    /// # Ok(()) }
    /// ```
    pub fn play(&mut self) -> Result<(), VideoError> {
        self.backend.play()
    }
    /// Pause playback, preserving position and the most recent frame texture.
    ///
    /// ```no_run
    /// # use macroquad::video::VideoPlayer;
    /// # fn open_menu(video: &mut VideoPlayer) {
    /// video.pause();
    /// # }
    /// ```
    pub fn pause(&mut self) {
        self.backend.pause();
    }
    /// Return to [`VideoState::Ready`] at position zero, dropping this player's
    /// frame texture and native decoder/audio resources. Browser media remains
    /// loaded. A later [`Self::play`] starts from the beginning.
    ///
    /// ```no_run
    /// # use macroquad::video::{VideoPlayer, VideoState};
    /// # fn reset(video: &mut VideoPlayer) {
    /// video.stop();
    /// assert_eq!(video.state(), VideoState::Ready);
    /// assert!(video.texture().is_none());
    /// # }
    /// ```
    pub fn stop(&mut self) {
        self.backend.stop();
        self.texture = None;
    }
    /// Set soundtrack volume, clamped to `0.0..=1.0`; ignore non-finite values.
    /// Native changes affect newly queued audio, not samples already submitted.
    ///
    /// ```no_run
    /// # use macroquad::video::VideoPlayer;
    /// # fn quiet(video: &mut VideoPlayer) {
    /// video.set_volume(0.25);
    /// # }
    /// ```
    pub fn set_volume(&mut self, volume: f32) {
        if volume.is_finite() {
            self.backend.set_volume(volume.clamp(0.0, 1.0));
        }
    }
    /// Mute or restore audio without changing the configured volume or clock.
    /// Native audio tracks still require an audio device while muted.
    ///
    /// ```no_run
    /// # use macroquad::video::VideoPlayer;
    /// # fn mute(video: &mut VideoPlayer) {
    /// video.set_muted(true);
    /// # }
    /// ```
    pub fn set_muted(&mut self, muted: bool) {
        self.backend.set_muted(muted);
    }
    /// Current playback position in seconds, using the audio clock when present.
    ///
    /// ```no_run
    /// # use macroquad::video::VideoPlayer;
    /// # fn show_time(video: &VideoPlayer) {
    /// let elapsed_seconds = video.position();
    /// # let _ = elapsed_seconds;
    /// # }
    /// ```
    pub fn position(&self) -> f64 {
        self.backend.position()
    }
    /// Clip duration in seconds, or `None` when usable duration is unavailable.
    ///
    /// ```no_run
    /// # use macroquad::video::VideoPlayer;
    /// # fn progress(video: &VideoPlayer) {
    /// if let Some(seconds) = video.duration() {
    ///     let fraction = (video.position() / seconds).clamp(0.0, 1.0);
    ///     # let _ = fraction;
    /// }
    /// # }
    /// ```
    pub fn duration(&self) -> Option<f64> {
        self.backend.duration()
    }
    /// Read the current playback state; call [`Self::update`] regularly to
    /// observe native decoder completion and asynchronous errors.
    ///
    /// ```no_run
    /// # use macroquad::video::{VideoPlayer, VideoState};
    /// # fn is_playing(video: &VideoPlayer) -> bool {
    /// video.state() == VideoState::Playing
    /// # }
    /// ```
    pub fn state(&self) -> VideoState {
        self.backend.state()
    }
    /// Most recent uploaded frame, or `None` before the first frame or after
    /// [`Self::stop`]. Pausing preserves it. Drawing does not advance playback.
    ///
    /// ```no_run
    /// # use macroquad::video::VideoPlayer;
    /// use macroquad::prelude::*;
    /// # fn draw_screen(video: &VideoPlayer) {
    /// if let Some(frame) = video.texture() {
    ///     draw_texture(frame, 20.0, 20.0, WHITE);
    /// }
    /// # }
    /// ```
    pub fn texture(&self) -> Option<&Texture2D> {
        self.texture.as_ref()
    }
    /// Advance decoding/audio and upload available frames on the macroquad
    /// thread. Call before drawing each frame. An error pauses playback;
    /// Ready players remain idle and do not allocate textures.
    ///
    /// ```no_run
    /// # use macroquad::video::{VideoError, VideoPlayer};
    /// # fn frame(video: &mut VideoPlayer) -> Result<(), VideoError> {
    /// video.update()?;
    /// // Submit draws using video.texture() after this call.
    /// # Ok(()) }
    /// ```
    pub fn update(&mut self) -> Result<(), VideoError> {
        let result = self.backend.update(&mut self.texture);
        if result.is_err() {
            self.backend.pause();
        }
        result
    }
}

#[cfg(any(not(target_arch = "wasm32"), test))]
fn fit_dimensions(
    width: u32,
    height: u32,
    options: VideoOptions,
) -> Result<(u16, u16), VideoError> {
    if width == 0 || height == 0 {
        return Err(VideoError("Video has no usable dimensions".into()));
    }
    let scale = (options.max_width as f64 / width as f64)
        .min(options.max_height as f64 / height as f64)
        .min(1.0);
    Ok((
        (width as f64 * scale).floor().max(1.0) as u16,
        (height as f64 * scale).floor().max(1.0) as u16,
    ))
}

fn upload(texture: &mut Option<Texture2D>, width: u16, height: u16, bytes: &[u8]) {
    if let Some(texture) = texture {
        texture.update_from_bytes(width as u32, height as u32, bytes);
    } else {
        let new = Texture2D::from_rgba8(width, height, bytes);
        new.set_filter(FilterMode::Linear);
        *texture = Some(new);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_preserve_portrait_aspect_and_never_upscale() {
        let o = VideoOptions {
            max_width: 540,
            max_height: 960,
            ..Default::default()
        };
        assert_eq!(fit_dimensions(1080, 1920, o).unwrap(), (540, 960));
        assert_eq!(fit_dimensions(320, 180, o).unwrap(), (320, 180));
        assert!(fit_dimensions(0, 100, o).is_err());
    }
    #[test]
    fn rejects_unbounded_or_invalid_decode_options() {
        for o in [
            VideoOptions {
                max_width: 0,
                ..Default::default()
            },
            VideoOptions {
                fps: 0,
                ..Default::default()
            },
            VideoOptions {
                max_height: 4097,
                ..Default::default()
            },
            VideoOptions {
                volume: f32::NAN,
                ..Default::default()
            },
        ] {
            assert!(o.validate().is_err());
        }
    }
}
