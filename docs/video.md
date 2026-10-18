# Video textures

Enable `macroquad = { ..., features = ["video"] }`, then use
`macroquad::video::{VideoPlayer, VideoOptions, VideoState}`. Video is independent
of the existing `audio` feature and keeps its soundtrack synchronized with its
texture. The example `examples/video_texture.rs` draws the texture in 2D; the
same `Texture2D` works on a 3D mesh or glTF screen surface.

```rust,ignore
let mut video = VideoPlayer::load("assets/clip.mp4", VideoOptions {
    max_width: 540, max_height: 960, ..Default::default()
}).await?;
video.play()?;
loop {
    video.update()?; // before rendering; advances/uploads available frames
    if let Some(texture) = video.texture() {
        // Draw this texture on your screen mesh using an unlit material.
    }
    next_frame().await;
}
```

`play()` starts/resumes and restarts an ended clip. `pause()` preserves position;
`stop()` returns to Ready/zero and releases native decoder resources. Native
playback starts lazily on `play()`. `set_muted(bool)` preserves the configured
volume, and `set_volume(0..1)` changes it. `update()` reports asynchronous
decoder/upload/autoplay errors and pauses playback on error. Dropping a player
releases all its resources. Dropping an unfinished load cancels metadata loading.
Load/play must run on the macroquad thread; decode work runs in workers.
Ready players do not allocate or upload frame textures. After `stop()`, repeated
`pause()`/`update()` calls remain Ready with no frame until the next `play()`.

## Browser

Load the companion plugin after the bundle and before WASM:

```html
<script src="mq_js_bundle.js"></script>
<script src="mq_video.js"></script>
<script>load("app.wasm");</script>
```

The browser streams, decodes and plays audio through one `HTMLVideoElement`.
Pixels upload directly from the media element (or a bounded resize canvas) to
the existing GL texture without copying through WASM memory. Decoded frame
counts skip duplicate uploads where supported. CORS must permit cross-origin
media; ordinary same-origin local HTTP assets need no additional configuration.
Codec support follows the browser. H.264/AAC MP4 is suitable for the experiment.
Use an explicit Play interaction for audible playback. Browsers can reject
autoplay; the readable error supports retry after interaction or muted playback.
A hidden tab pauses media and requires explicit resume. GPU binding/unpack state
is restored after upload. The browser manages its compressed media cache.

## Native Windows

Install FFmpeg including FFprobe, either on PATH or configure full executable
paths in `MACROQUAD_FFMPEG` and `MACROQUAD_FFPROBE`. Keep their dependent DLLs
beside the executables if using a shared build. The library never downloads or
installs a decoder. Native sources must be readable local files. Metadata probing
has a 10-second deadline and kills/reaps the subprocess on timeout or cancellation.

Two FFmpeg processes stream scaled RGBA frames and stereo 48 kHz signed PCM.
The Windows WinMM output device's consumed sample position drives frame
presentation, including during mute and pause. Audio underflow freezes this
clock; video catches up to audio by dropping obsolete frames. Silent videos use
elapsed active time. Native decoding normalizes frames to the requested `fps`.
There are three queued video frames, a pending presentation frame and transient
decode/upload frames; memory use is independent of clip length. Audio uses eight
2048-sample packets plus four output buffers (under 300 ms at 48 kHz stereo).
Volume/mute changes apply to newly queued buffers, so they can take about 85 ms.
Call `update` regularly to refill audio. Stop/drop closes the device, kills and
reaps processes, and unblocks pipe readers/producers. A default audio device is
required for clips with a soundtrack, even when muted.

Current native audio support is Windows. Other native platforms can play silent
video files but return a clear error for audio tracks. Browser audio works on
platforms with supported media codecs. The initial API targets finite local
clips and does not implement seeking, spatial audio, subtitles, live streams,
or seamless native looping. Native loops restart decoding at the end. The API
does not promise frame-accurate editing or color-managed HDR playback.

## Checks

```text
cargo test --features video --lib video::
node js/mq_video.test.cjs
cargo check --features video --target wasm32-unknown-unknown
cargo run --features video --example video_texture -- path/to/clip.mp4
```

The native integration test is opt-in: set `MACROQUAD_VIDEO_TEST_FILE` to a local
clip and the FFmpeg paths, then run
`cargo test --features video --lib native_decoder_lifecycle -- --ignored`.
It opens a real audio output, advances frames without a GPU, verifies pause and
resume, and checks that stop releases decoder processes.
