use super::{fit_dimensions, upload, Texture2D, VideoError, VideoOptions, VideoState};
#[cfg(target_os = "windows")]
#[path = "wave_audio.rs"]
mod wave_audio;
#[cfg(target_os = "windows")]
use wave_audio::AudioOutput;
#[cfg(not(target_os = "windows"))]
mod unsupported_audio {
    use super::*;
    pub struct AudioOutput;
    impl AudioOutput {
        pub fn open(_: Receiver<Vec<i16>>, _: f32) -> Result<Self, VideoError> {
            Err(VideoError("Native video audio currently requires Windows. Web playback supports browser audio.".into()))
        }
        pub fn play(&self) {}
        pub fn pause(&self) {}
        pub fn set_volume(&mut self, _: f32) {}
        pub fn position(&self) -> f64 {
            0.0
        }
        pub fn pump(&mut self) -> Result<(), VideoError> {
            Ok(())
        }
        pub fn ended(&self) -> bool {
            true
        }
    }
}
use std::{
    io::Read,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
        Arc,
    },
    time::{Duration, Instant},
};
#[cfg(not(target_os = "windows"))]
use unsupported_audio::AudioOutput;

const AUDIO_PACKET_SAMPLES: usize = 2048;
const AUDIO_QUEUE_PACKETS: usize = 8;
const VIDEO_QUEUE_FRAMES: usize = 3;

fn command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    cmd.stdin(Stdio::null()).stderr(Stdio::null());
    cmd
}
fn executable(variable: &str, fallback: &str) -> String {
    std::env::var(variable).unwrap_or_else(|_| fallback.into())
}

struct Metadata {
    width: u16,
    height: u16,
    duration: Option<f64>,
    audio: bool,
}
struct CancelProbe(Arc<AtomicBool>);
impl Drop for CancelProbe {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}
fn probe(
    path: &str,
    options: VideoOptions,
    cancel: Arc<AtomicBool>,
) -> Result<Metadata, VideoError> {
    if !std::fs::metadata(path).is_ok_and(|m| m.is_file()) {
        return Err(VideoError(format!(
            "Native video needs a readable local file: {path}"
        )));
    }
    let mut child = command(&executable("MACROQUAD_FFPROBE", "ffprobe"))
        .args([
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
            path,
        ])
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| {
            VideoError(format!(
                "Cannot run ffprobe: {e}. Install FFmpeg or set MACROQUAD_FFPROBE."
            ))
        })?;
    let mut pipe = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut data = Vec::new();
        let mut chunk = [0u8; 4096];
        while let Ok(n) = pipe.read(&mut chunk) {
            if n == 0 {
                break;
            }
            // Metadata is bounded even for malformed/unusually large files.
            if data.len() < 1024 * 1024 {
                data.extend_from_slice(&chunk[..n]);
            }
        }
        data
    });
    let started = Instant::now();
    let status = loop {
        if cancel.load(Ordering::Relaxed) || started.elapsed() > Duration::from_secs(10) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err(VideoError("Video metadata cancelled or timed out".into()));
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err(VideoError(format!(
                    "Video metadata process failed: {error}"
                )));
            }
        }
    };
    let output = reader
        .join()
        .map_err(|_| VideoError("Video metadata reader failed".into()))?;
    if !status.success() {
        return Err(VideoError(format!("ffprobe cannot open video: {path}")));
    }
    let json: serde_json::Value = serde_json::from_slice(&output)
        .map_err(|e| VideoError(format!("Invalid ffprobe metadata: {e}")))?;
    let streams = json["streams"]
        .as_array()
        .ok_or_else(|| VideoError("Video has no streams".into()))?;
    let video = streams
        .iter()
        .find(|s| s["codec_type"] == "video")
        .ok_or_else(|| VideoError("File has no video stream".into()))?;
    let mut width = video["width"].as_u64().unwrap_or(0) as u32;
    let mut height = video["height"].as_u64().unwrap_or(0) as u32;
    // FFmpeg autorotates smartphone footage before applying the scale filter.
    let rotation = video["side_data_list"]
        .as_array()
        .and_then(|items| items.iter().find_map(|s| s["rotation"].as_i64()))
        .unwrap_or(0);
    if rotation.abs() % 180 == 90 {
        std::mem::swap(&mut width, &mut height);
    }
    let (width, height) = fit_dimensions(width, height, options)?;
    let duration = json["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|n| n.is_finite() && *n > 0.0);
    Ok(Metadata {
        width,
        height,
        duration,
        audio: streams.iter().any(|s| s["codec_type"] == "audio"),
    })
}

struct Frame {
    index: u64,
    bytes: Vec<u8>,
}

struct Runtime {
    video: Child,
    audio: Option<Child>,
    frames: Option<Receiver<Frame>>,
    pending: Option<Frame>,
    frames_ended: bool,
    last_frame: u64,
    sink: Option<AudioOutput>,
    tail: Option<(f64, Instant)>,
    wall_position: f64,
    wall_start: Option<Instant>,
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Runtime {
    fn shutdown(&mut self) {
        // Dropping receivers releases bounded senders, killing children releases
        // pipe readers, and wait reaps OS resources. Nothing is detached alive.
        self.sink.take();
        self.frames.take();
        let _ = self.video.kill();
        let _ = self.video.wait();
        if let Some(audio) = &mut self.audio {
            let _ = audio.kill();
            let _ = audio.wait();
        }
    }

    fn start(
        path: &str,
        options: VideoOptions,
        metadata: &Metadata,
        muted: bool,
    ) -> Result<Self, VideoError> {
        let ffmpeg = executable("MACROQUAD_FFMPEG", "ffmpeg");
        // Open audio first so an unavailable device cannot leak a child process.
        let (audio_tx, audio_rx) = mpsc::sync_channel(AUDIO_QUEUE_PACKETS);
        let sink = if metadata.audio {
            Some(AudioOutput::open(
                audio_rx,
                if muted { 0.0 } else { options.volume },
            )?)
        } else {
            None
        };
        let filter = format!(
            "fps={}:start_time=0,scale={}:{}",
            options.fps, metadata.width, metadata.height
        );
        let mut video = command(&ffmpeg)
            .args([
                "-v", "error", "-nostdin", "-i", path, "-map", "0:v:0", "-an", "-vf", &filter,
                "-pix_fmt", "rgba", "-f", "rawvideo", "pipe:1",
            ])
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| {
                VideoError(format!(
                    "Cannot run ffmpeg: {e}. Install FFmpeg or set MACROQUAD_FFMPEG."
                ))
            })?;
        let mut stdout = video.stdout.take().unwrap();
        let (tx, frames) = mpsc::sync_channel(VIDEO_QUEUE_FRAMES);
        let size = metadata.width as usize * metadata.height as usize * 4;
        std::thread::spawn(move || {
            let mut index = 0;
            loop {
                let mut bytes = vec![0; size];
                if stdout.read_exact(&mut bytes).is_err() {
                    break;
                }
                if tx.send(Frame { index, bytes }).is_err() {
                    break;
                }
                index += 1;
            }
        });
        let mut runtime = Self {
            video,
            audio: None,
            frames: Some(frames),
            pending: None,
            frames_ended: false,
            last_frame: 0,
            sink,
            tail: None,
            wall_position: 0.0,
            wall_start: None,
        };
        if runtime.sink.is_some() {
            let mut child = command(&ffmpeg)
                .args([
                    "-v",
                    "error",
                    "-nostdin",
                    "-i",
                    path,
                    "-map",
                    "0:a:0",
                    "-vn",
                    "-af",
                    "aresample=48000:async=1:first_pts=0",
                    "-ac",
                    "2",
                    "-ar",
                    "48000",
                    "-f",
                    "s16le",
                    "pipe:1",
                ])
                .stdout(Stdio::piped())
                .spawn()
                .map_err(|e| VideoError(format!("Cannot start FFmpeg audio: {e}")))?;
            let mut stdout = child.stdout.take().unwrap();
            runtime.audio = Some(child);
            let tx = audio_tx;
            std::thread::spawn(move || {
                let mut bytes = vec![0; AUDIO_PACKET_SAMPLES * 2];
                loop {
                    let mut count = 0;
                    while count < bytes.len() {
                        match stdout.read(&mut bytes[count..]) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => count += n,
                        }
                    }
                    if count == 0 {
                        break;
                    }
                    let packet = bytes[..count]
                        .chunks_exact(2)
                        .map(|s| i16::from_le_bytes([s[0], s[1]]))
                        .collect();
                    if tx.send(packet).is_err() || count != bytes.len() {
                        break;
                    }
                }
            });
        }
        Ok(runtime)
    }
    fn position(&self) -> f64 {
        if self.sink.is_some() {
            self.tail
                .map(|(base, start)| {
                    base + if self.wall_start.is_some() {
                        start.elapsed().as_secs_f64()
                    } else {
                        0.0
                    }
                })
                .unwrap_or_else(|| self.sink.as_ref().unwrap().position())
        } else {
            self.wall_position
                + self
                    .wall_start
                    .map(|t| t.elapsed().as_secs_f64())
                    .unwrap_or(0.0)
        }
    }
    fn play(&mut self) {
        if self.wall_start.is_none() {
            self.wall_start = Some(Instant::now());
            if let Some((_, start)) = &mut self.tail {
                *start = Instant::now();
            }
            if let Some(sink) = &self.sink {
                sink.play();
            }
        }
    }
    fn pause(&mut self) {
        let pos = self.position();
        if let Some(sink) = &self.sink {
            sink.pause();
        }
        if self.tail.is_some() {
            self.tail = Some((pos, Instant::now()));
        }
        self.wall_position = pos;
        self.wall_start = None;
    }
    fn check_process(child: &mut Child, label: &str) -> Result<(), VideoError> {
        if let Some(status) = child.try_wait().map_err(|e| VideoError(e.to_string()))? {
            if !status.success() {
                return Err(VideoError(format!(
                    "FFmpeg {label} decoder failed ({status})"
                )));
            }
        }
        Ok(())
    }
}

pub(super) struct Backend {
    path: String,
    options: VideoOptions,
    metadata: Metadata,
    runtime: Option<Runtime>,
    state: VideoState,
    muted: bool,
    ended_position: f64,
}
impl Backend {
    pub async fn load(path: &str, options: VideoOptions) -> Result<Self, VideoError> {
        let probe_path = path.to_owned();
        let (tx, rx) = mpsc::channel();
        let cancellation = CancelProbe(Arc::new(AtomicBool::new(false)));
        let cancel = cancellation.0.clone();
        std::thread::spawn(move || {
            let _ = tx.send(probe(&probe_path, options, cancel));
        });
        let started = Instant::now();
        let metadata = loop {
            match rx.try_recv() {
                Ok(result) => break result?,
                Err(TryRecvError::Disconnected) => {
                    return Err(VideoError("Video probe worker stopped".into()))
                }
                Err(TryRecvError::Empty) => {
                    if started.elapsed() > Duration::from_secs(30) {
                        return Err(VideoError("Video metadata timed out".into()));
                    }
                    crate::window::next_frame().await;
                }
            }
        };
        Ok(Self {
            path: path.into(),
            options,
            metadata,
            runtime: None,
            state: VideoState::Ready,
            muted: false,
            ended_position: 0.0,
        })
    }
    pub fn play(&mut self) -> Result<(), VideoError> {
        if self.state == VideoState::Playing {
            return Ok(());
        }
        if self.runtime.is_none() {
            self.runtime = Some(Runtime::start(
                &self.path,
                self.options,
                &self.metadata,
                self.muted,
            )?);
        }
        self.runtime.as_mut().unwrap().play();
        self.state = VideoState::Playing;
        Ok(())
    }
    pub fn pause(&mut self) {
        if self.state == VideoState::Playing {
            if let Some(runtime) = &mut self.runtime {
                runtime.pause();
            }
            self.state = VideoState::Paused;
        }
    }
    pub fn stop(&mut self) {
        self.runtime = None;
        self.ended_position = 0.0;
        self.state = VideoState::Ready;
    }
    pub fn set_volume(&mut self, volume: f32) {
        self.options.volume = volume;
        if let Some(runtime) = &mut self.runtime {
            if let Some(sink) = &mut runtime.sink {
                sink.set_volume(if self.muted { 0.0 } else { volume });
            }
        }
    }
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        self.set_volume(self.options.volume);
    }
    pub fn position(&self) -> f64 {
        self.runtime
            .as_ref()
            .map(Runtime::position)
            .unwrap_or(self.ended_position)
    }
    pub fn duration(&self) -> Option<f64> {
        self.metadata.duration
    }
    pub fn state(&self) -> VideoState {
        self.state
    }
    pub fn update(&mut self, texture: &mut Option<Texture2D>) -> Result<(), VideoError> {
        let Some(runtime) = &mut self.runtime else {
            return Ok(());
        };
        Runtime::check_process(&mut runtime.video, "video")?;
        if let Some(audio) = &mut runtime.audio {
            Runtime::check_process(audio, "audio")?;
        }
        if let Some(sink) = &mut runtime.sink {
            sink.pump()?;
        }
        if runtime.sink.as_ref().is_some_and(|s| s.ended()) && runtime.tail.is_none() {
            runtime.tail = Some((runtime.position(), Instant::now()));
        }
        let position = runtime.position();
        let mut chosen = None;
        loop {
            if runtime.pending.is_none() && !runtime.frames_ended {
                match runtime.frames.as_ref().unwrap().try_recv() {
                    Ok(frame) => runtime.pending = Some(frame),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => runtime.frames_ended = true,
                }
            }
            if runtime
                .pending
                .as_ref()
                .is_some_and(|f| f.index as f64 / self.options.fps as f64 <= position)
            {
                chosen = runtime.pending.take();
                runtime.last_frame = chosen.as_ref().unwrap().index;
            } else {
                break;
            }
        }
        if let Some(frame) = chosen {
            upload(
                texture,
                self.metadata.width,
                self.metadata.height,
                &frame.bytes,
            );
        }
        let video_end = (runtime.last_frame + 1) as f64 / self.options.fps as f64;
        if runtime.frames_ended
            && runtime.pending.is_none()
            && position >= video_end
            && (runtime.sink.is_none() || runtime.sink.as_ref().is_some_and(|s| s.ended()))
        {
            self.ended_position = position;
            self.runtime = None;
            self.state = VideoState::Ended;
            if self.options.looped {
                self.play()?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stopped_video_stays_ready_without_allocating_a_texture() {
        // This test intentionally has no graphics context: an attempted texture
        // upload/allocation after stop would panic instead of silently passing.
        let mut backend = Backend {
            path: "not-opened-after-stop.mp4".into(),
            options: VideoOptions::default(),
            metadata: Metadata {
                width: 1,
                height: 1,
                duration: Some(1.0),
                audio: false,
            },
            runtime: None,
            state: VideoState::Ended,
            muted: false,
            ended_position: 1.0,
        };
        backend.stop();
        let mut texture = None;
        for _ in 0..5 {
            backend.pause();
            backend.update(&mut texture).unwrap();
            assert_eq!(backend.state(), VideoState::Ready);
            assert_eq!(backend.position(), 0.0);
            assert!(texture.is_none());
            assert!(backend.runtime.is_none());
        }
    }

    #[test]
    fn decoded_queues_are_bounded_and_shutdown_unblocks_producers() {
        let (tx, rx) = mpsc::sync_channel(VIDEO_QUEUE_FRAMES);
        for i in 0..VIDEO_QUEUE_FRAMES {
            tx.try_send(i).unwrap();
        }
        assert!(matches!(tx.try_send(99), Err(mpsc::TrySendError::Full(99))));
        drop(rx);
        assert!(matches!(tx.send(99), Err(mpsc::SendError(99))));
        assert!(AUDIO_QUEUE_PACKETS * AUDIO_PACKET_SAMPLES < 48_000 * 2);
    }

    #[test]
    #[ignore = "Requires a local MP4, FFmpeg and a Windows audio device; set MACROQUAD_VIDEO_TEST_FILE"]
    #[cfg(target_os = "windows")]
    fn native_decoder_lifecycle() {
        let path =
            std::env::var("MACROQUAD_VIDEO_TEST_FILE").expect("Set MACROQUAD_VIDEO_TEST_FILE");
        let options = VideoOptions {
            max_width: 180,
            max_height: 320,
            ..Default::default()
        };
        let metadata = probe(&path, options, Arc::new(AtomicBool::new(false))).unwrap();
        let mut runtime = Runtime::start(&path, options, &metadata, true).unwrap();
        runtime.play();
        let start = Instant::now();
        let mut frames = 0;
        let mut first = None;
        let mut changed = false;
        while start.elapsed() < Duration::from_secs(2) {
            if let Some(sink) = &mut runtime.sink {
                sink.pump().unwrap();
            }
            while let Ok(frame) = runtime.frames.as_ref().unwrap().try_recv() {
                if let Some(bytes) = &first {
                    changed |= *bytes != frame.bytes;
                } else {
                    first = Some(frame.bytes);
                }
                frames += 1;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(frames >= 10, "Decoder must stream multiple frames");
        assert!(changed, "The test clip must have moving imagery");
        assert!(
            runtime.position() > 0.4,
            "Device clock must advance even when muted"
        );
        runtime.pause();
        let paused = runtime.position();
        std::thread::sleep(Duration::from_millis(150));
        assert!(
            (runtime.position() - paused).abs() < 0.03,
            "Pause freezes media clock"
        );
        runtime.play();
        let resume = Instant::now();
        while resume.elapsed() < Duration::from_millis(200) {
            if let Some(sink) = &mut runtime.sink {
                sink.pump().unwrap();
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            runtime.position() > paused + 0.05,
            "Resume continues media clock"
        );
        runtime.shutdown();
        assert!(
            runtime.video.try_wait().unwrap().is_some(),
            "Video process is reaped"
        );
        if let Some(audio) = &mut runtime.audio {
            assert!(
                audio.try_wait().unwrap().is_some(),
                "Audio process is reaped"
            );
        }
        assert!(runtime.sink.is_none());
        assert!(runtime.frames.is_none());
        assert!(
            probe(&path, options, Arc::new(AtomicBool::new(true))).is_err(),
            "Cancelled probe must terminate"
        );
    }
}
