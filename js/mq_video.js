// Optional macroquad `video` feature. Load after mq_js_bundle.js, before load().
// Browser decoding/audio share HTMLMediaElement's clock; frames never cross WASM.
(function () {
    "use strict";
    const videos = new Map();
    let nextId = 1;
    function fail(record, error) {
        record.error = error && error.message ? error.message : String(error);
        record.state = 2;
        record.generation++;
        record.video.pause();
    }
    function create(pointer, length, maxWidth, maxHeight, looped, volume) {
        const id = nextId++;
        const video = document.createElement("video");
        const record = { video, width: 0, height: 0, error: "", state: 0,
            generation: 0, lastTime: -1, lastFrame: -1, canvas: null, context: null };
        videos.set(id, record);
        video.preload = "auto";
        video.playsInline = true;
        video.crossOrigin = "anonymous";
        video.loop = !!looped;
        video.volume = volume;
        video.addEventListener("loadedmetadata", () => {
            if (!videos.has(id)) return;
            const scale = Math.min(1, maxWidth / video.videoWidth, maxHeight / video.videoHeight);
            record.width = Math.max(1, Math.floor(video.videoWidth * scale));
            record.height = Math.max(1, Math.floor(video.videoHeight * scale));
            if (record.width !== video.videoWidth || record.height !== video.videoHeight) {
                record.canvas = document.createElement("canvas");
                record.canvas.width = record.width;
                record.canvas.height = record.height;
                record.context = record.canvas.getContext("2d", { alpha: false });
                if (!record.context) fail(record, "Video resize canvas is unavailable");
            }
        });
        video.addEventListener("error", () => fail(record,
            `Video failed to load/decode (media code ${video.error ? video.error.code : "unknown"}). Check URL, codec and CORS.`));
        video.addEventListener("ended", () => { record.state = 3; });
        video.addEventListener("pause", () => { if (record.state === 1) record.state = 2; });
        video.src = UTF8ToString(pointer, length);
        video.load();
        return id;
    }
    function play(id) {
        const record = videos.get(id);
        if (!record) return;
        record.error = "";
        if (record.video.ended) record.video.currentTime = 0;
        record.state = 1;
        const generation = ++record.generation;
        try {
            const request = record.video.play();
            if (request && request.catch) request.catch(error => {
                if (!videos.has(id) || generation !== record.generation) return;
                fail(record, error.name === "NotAllowedError"
                    ? "Browser blocked video audio. Press Play again after interacting with the page, or mute the video."
                    : error);
            });
        } catch (error) { fail(record, error); }
    }
    function pause(id) {
        const record = videos.get(id);
        if (!record) return;
        record.generation++;
        // A game may pause while the TV is stopped or already ended. Preserve
        // those states; otherwise its next update recreates a stopped texture.
        if (record.state === 1) record.state = 2;
        record.video.pause();
    }
    function stop(id) {
        const record = videos.get(id);
        if (!record) return;
        pause(id);
        record.video.currentTime = 0;
        record.lastTime = -1;
        record.lastFrame = -1;
        record.state = 0;
    }
    function upload(id, textureId) {
        const record = videos.get(id);
        if (!record || record.state === 0 || record.video.readyState < 2 || !record.width) return 0;
        const quality = record.video.getVideoPlaybackQuality ? record.video.getVideoPlaybackQuality() : null;
        const frame = quality && quality.totalVideoFrames > 0 ? quality.totalVideoFrames : -1;
        if (frame >= 0 ? record.lastFrame === frame : record.lastTime === record.video.currentTime) return 0;
        const binding = gl.getParameter(gl.TEXTURE_BINDING_2D);
        const flip = gl.getParameter(gl.UNPACK_FLIP_Y_WEBGL);
        const premultiplied = gl.getParameter(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL);
        try {
            gl.bindTexture(gl.TEXTURE_2D, GL.textures[textureId]);
            gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
            gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, false);
            let source = record.video;
            if (record.canvas) {
                record.context.drawImage(record.video, 0, 0, record.width, record.height);
                source = record.canvas;
            }
            gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, gl.RGBA, gl.UNSIGNED_BYTE, source);
            record.lastTime = record.video.currentTime;
            record.lastFrame = frame;
            return 1;
        } catch (error) { fail(record, `Video texture upload failed: ${error.message || error}`); return 0; }
        finally {
            gl.bindTexture(gl.TEXTURE_2D, binding);
            gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, flip);
            gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, premultiplied);
        }
    }
    function remove(id) {
        const record = videos.get(id);
        if (!record) return;
        record.generation++;
        videos.delete(id);
        record.video.pause();
        record.video.removeAttribute("src");
        record.video.load();
        record.canvas = null;
        record.context = null;
    }
    // Do not keep audio playing when the app stops presenting frames in a hidden
    // tab. Resume requires a deliberate user action, keeping UX predictable.
    document.addEventListener("visibilitychange", () => {
        if (document.hidden) for (const [id, record] of videos) {
            if (record.state === 1) pause(id);
        }
    });
    miniquad_add_plugin({ name: "macroquad_video", version: 1, register_plugin(imports) {
        Object.assign(imports.env, {
            mq_video_create: create,
            mq_video_ready: id => {
                const r = videos.get(id);
                // 1 means metadata only; 2 means a frame is presentable. A
                // stopped player must not ask Rust to allocate/upload a texture.
                return r && r.width ? (r.state !== 0 && r.video.readyState >= 2 ? 2 : 1) : 0;
            },
            mq_video_width: id => videos.get(id).width,
            mq_video_height: id => videos.get(id).height,
            mq_video_play: play, mq_video_pause: pause, mq_video_stop: stop,
            mq_video_volume: (id, volume) => { videos.get(id).video.volume = volume; },
            mq_video_muted: (id, muted) => { videos.get(id).video.muted = !!muted; },
            mq_video_position: id => videos.get(id).video.currentTime,
            mq_video_duration: id => videos.get(id).video.duration,
            mq_video_state: id => videos.get(id).state,
            mq_video_upload: upload,
            mq_video_error: (id, pointer, length) => {
                const message = videos.has(id) ? videos.get(id).error : "Video was disposed";
                const bytes = new TextEncoder().encode(message).subarray(0, length);
                new Uint8Array(wasm_memory.buffer, pointer, bytes.length).set(bytes);
                return bytes.length;
            },
            mq_video_delete: remove,
        });
    }});
}());
