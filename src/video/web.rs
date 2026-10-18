use super::{Texture2D, VideoError, VideoOptions, VideoState};

extern "C" {
    fn mq_video_create(
        path: *const u8,
        len: usize,
        width: u32,
        height: u32,
        looped: u32,
        volume: f32,
    ) -> u32;
    fn mq_video_ready(id: u32) -> u32;
    fn mq_video_width(id: u32) -> u32;
    fn mq_video_height(id: u32) -> u32;
    fn mq_video_play(id: u32);
    fn mq_video_pause(id: u32);
    fn mq_video_stop(id: u32);
    fn mq_video_volume(id: u32, volume: f32);
    fn mq_video_muted(id: u32, muted: u32);
    fn mq_video_position(id: u32) -> f64;
    fn mq_video_duration(id: u32) -> f64;
    fn mq_video_state(id: u32) -> u32;
    fn mq_video_upload(id: u32, texture: u32) -> u32;
    fn mq_video_error(id: u32, buffer: *mut u8, len: usize) -> usize;
    fn mq_video_delete(id: u32);
}
#[no_mangle]
extern "C" fn macroquad_video_crate_version() -> u32 {
    1
}

pub(super) struct Backend {
    id: u32,
    width: u16,
    height: u16,
}
impl Drop for Backend {
    fn drop(&mut self) {
        unsafe {
            mq_video_delete(self.id);
        }
    }
}
impl Backend {
    pub async fn load(path: &str, options: VideoOptions) -> Result<Self, VideoError> {
        let id = unsafe {
            mq_video_create(
                path.as_ptr(),
                path.len(),
                options.max_width as u32,
                options.max_height as u32,
                options.looped as u32,
                options.volume,
            )
        };
        if id == 0 {
            return Err(VideoError(
                "Video plugin unavailable: load mq_video.js before the WASM application".into(),
            ));
        }
        let mut backend = Self {
            id,
            width: 0,
            height: 0,
        };
        let started = crate::time::get_time();
        while unsafe { mq_video_ready(id) } == 0 {
            backend.check_error()?;
            if crate::time::get_time() - started > 30.0 {
                return Err(VideoError("Video metadata timed out".into()));
            }
            crate::window::next_frame().await;
        }
        backend.width = unsafe { mq_video_width(id) } as u16;
        backend.height = unsafe { mq_video_height(id) } as u16;
        Ok(backend)
    }
    fn check_error(&self) -> Result<(), VideoError> {
        let mut message = [0u8; 1024];
        let len = unsafe { mq_video_error(self.id, message.as_mut_ptr(), message.len()) }
            .min(message.len());
        if len > 0 {
            Err(VideoError(
                String::from_utf8_lossy(&message[..len]).into_owned(),
            ))
        } else {
            Ok(())
        }
    }
    pub fn play(&mut self) -> Result<(), VideoError> {
        unsafe {
            mq_video_play(self.id);
        }
        self.check_error()
    }
    pub fn pause(&mut self) {
        unsafe {
            mq_video_pause(self.id);
        }
    }
    pub fn stop(&mut self) {
        unsafe {
            mq_video_stop(self.id);
        }
    }
    pub fn set_volume(&mut self, volume: f32) {
        unsafe {
            mq_video_volume(self.id, volume);
        }
    }
    pub fn set_muted(&mut self, muted: bool) {
        unsafe {
            mq_video_muted(self.id, muted as u32);
        }
    }
    pub fn position(&self) -> f64 {
        unsafe { mq_video_position(self.id) }
    }
    pub fn duration(&self) -> Option<f64> {
        let n = unsafe { mq_video_duration(self.id) };
        if n.is_finite() && n > 0.0 {
            Some(n)
        } else {
            None
        }
    }
    pub fn state(&self) -> VideoState {
        match unsafe { mq_video_state(self.id) } {
            1 => VideoState::Playing,
            2 => VideoState::Paused,
            3 => VideoState::Ended,
            _ => VideoState::Ready,
        }
    }
    pub fn update(&mut self, texture: &mut Option<Texture2D>) -> Result<(), VideoError> {
        self.check_error()?;
        // Loading metadata and stopping must not allocate a frame texture.
        // In particular, stop() drops the old texture and remains GPU-idle
        // until play() is called again, matching the native backend.
        if self.state() == VideoState::Ready {
            return Ok(());
        }
        if unsafe { mq_video_ready(self.id) } < 2 {
            return Ok(());
        }
        if texture.is_none() {
            super::upload(
                texture,
                self.width,
                self.height,
                &vec![0; self.width as usize * self.height as usize * 4],
            );
        }
        let texture = texture.as_ref().unwrap();
        // Flush before mutating pixels so already-submitted draws retain their
        // frame. The plugin restores GL binding and pixel unpack state.
        let ctx = crate::get_context();
        ctx.perform_render_passes();
        let raw = unsafe { crate::get_quad_context().texture_raw_id(texture.raw_miniquad_id()) };
        let miniquad::RawId::OpenGl(raw) = raw;
        unsafe {
            mq_video_upload(self.id, raw);
        }
        self.check_error()
    }
}
