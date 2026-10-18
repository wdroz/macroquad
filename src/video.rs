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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoError(pub String);
impl std::fmt::Display for VideoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for VideoError {}

#[derive(Debug, Clone, Copy)]
pub struct VideoOptions {
    /// Decoded texture bounds; aspect ratio is preserved without upscaling.
    pub max_width: u16,
    pub max_height: u16,
    /// Native decode rate. The browser uses the source's presentation rate.
    pub fps: u16,
    pub looped: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoState {
    Ready,
    Playing,
    Paused,
    Ended,
}

/// Owns decoder/audio resources. Dropping or stopping releases native decoder
/// processes; dropping also releases browser media and GPU texture resources.
/// Call `update` before drawing each frame. The returned texture can be used by
/// any normal macroquad 2D/3D mesh, including an emissive screen material.
pub struct VideoPlayer {
    backend: Backend,
    texture: Option<Texture2D>,
}
impl VideoPlayer {
    pub async fn load(path: &str, options: VideoOptions) -> Result<Self, VideoError> {
        let options = options.validate()?;
        let backend = Backend::load(path, options).await?;
        Ok(Self {
            backend,
            texture: None,
        })
    }
    pub fn play(&mut self) -> Result<(), VideoError> {
        self.backend.play()
    }
    pub fn pause(&mut self) {
        self.backend.pause();
    }
    pub fn stop(&mut self) {
        self.backend.stop();
        self.texture = None;
    }
    pub fn set_volume(&mut self, volume: f32) {
        if volume.is_finite() {
            self.backend.set_volume(volume.clamp(0.0, 1.0));
        }
    }
    pub fn set_muted(&mut self, muted: bool) {
        self.backend.set_muted(muted);
    }
    pub fn position(&self) -> f64 {
        self.backend.position()
    }
    pub fn duration(&self) -> Option<f64> {
        self.backend.duration()
    }
    pub fn state(&self) -> VideoState {
        self.backend.state()
    }
    pub fn texture(&self) -> Option<&Texture2D> {
        self.texture.as_ref()
    }
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
