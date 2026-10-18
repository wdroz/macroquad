// Run with: node js/mq_video.test.cjs
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');

async function main() {
    let plugin, uploads = 0, scaled = 0;
    const media = [];
    const events = {};
    const memory = { buffer: new ArrayBuffer(4096) };
    const initialTexture = { name: 'previous texture' };
    const state = new Map([[1, initialTexture], [2, true], [3, true]]);
    const gl = {
        TEXTURE_BINDING_2D: 1, UNPACK_FLIP_Y_WEBGL: 2, UNPACK_PREMULTIPLY_ALPHA_WEBGL: 3,
        TEXTURE_2D: 4, RGBA: 5, UNSIGNED_BYTE: 6,
        getParameter: k => state.get(k), bindTexture: (_, t) => state.set(1, t),
        pixelStorei: (k, v) => state.set(k, v),
        texSubImage2D: () => { uploads++; },
    };
    const document = {
        hidden: false,
        addEventListener: (name, callback) => { events[name] = callback; },
        createElement(name) {
            if (name === 'canvas') return { getContext: () => ({ drawImage() { scaled++; } }) };
            assert.equal(name, 'video');
            const listeners = {};
            const video = {
                currentTime: 0, duration: 29.4, readyState: 0, videoWidth: 1080, videoHeight: 1920,
                paused: true, ended: false, muted: false, decoded: 1, loads: 0,
                addEventListener: (name, callback) => { listeners[name] = callback; },
                load() { this.loads++; },
                play() { this.paused = false; return this.reject ? Promise.reject(this.reject) : Promise.resolve(); },
                pause() { this.paused = true; if (listeners.pause) listeners.pause(); },
                removeAttribute(name) { delete this[name]; },
                fire: name => listeners[name](),
                getVideoPlaybackQuality() { return { totalVideoFrames: this.decoded }; },
            };
            media.push(video);
            return video;
        },
    };
    const context = vm.createContext({ document, gl, GL: { textures: [{}, { name: 'video' }] },
        wasm_memory: memory, TextEncoder, Uint8Array, console,
        UTF8ToString: () => 'clip.mp4', miniquad_add_plugin: p => { plugin = p; } });
    vm.runInContext(fs.readFileSync(path.join(__dirname, 'mq_video.js'), 'utf8'), context);
    assert.equal(plugin.name, 'macroquad_video');
    const imports = { env: {} }; plugin.register_plugin(imports);
    const api = imports.env;
    const id = api.mq_video_create(0, 0, 540, 960, 0, 0.7);
    const video = media[0];
    assert.equal(api.mq_video_ready(id), 0);
    video.readyState = 2; video.fire('loadedmetadata');
    assert.equal(api.mq_video_ready(id), 1, 'metadata alone must not request a GPU frame');
    assert.equal(api.mq_video_width(id), 540);
    assert.equal(api.mq_video_height(id), 960);
    api.mq_video_play(id);
    assert.equal(api.mq_video_state(id), 1);
    assert.equal(api.mq_video_ready(id), 2);
    assert.equal(api.mq_video_upload(id, 1), 1);
    video.currentTime = 0.01; // clock changed but decoded frame did not
    assert.equal(api.mq_video_upload(id, 1), 0);
    video.decoded++;
    assert.equal(api.mq_video_upload(id, 1), 1);
    assert.equal(uploads, 2); assert.equal(scaled, 2);
    assert.equal(state.get(1), initialTexture);
    assert.equal(state.get(2), true); assert.equal(state.get(3), true);
    api.mq_video_muted(id, 1); assert.equal(video.muted, true);
    api.mq_video_volume(id, 0.2); assert.equal(video.volume, 0.2);
    video.currentTime = 3;
    api.mq_video_pause(id); assert.equal(api.mq_video_state(id), 2);
    assert.equal(api.mq_video_position(id), 3);
    api.mq_video_play(id); document.hidden = true; events.visibilitychange();
    assert.equal(video.paused, true); assert.equal(api.mq_video_state(id), 2);
    api.mq_video_stop(id); assert.equal(video.currentTime, 0); assert.equal(api.mq_video_state(id), 0);
    const uploadsBeforeStop = uploads;
    for (let i = 0; i < 5; i++) {
        api.mq_video_pause(id); // leaving café / game pause after reset
        assert.equal(api.mq_video_state(id), 0, 'pause preserves stopped/Ready state');
        assert.equal(api.mq_video_ready(id), 1, 'stop must not recreate a GPU texture on later updates');
        assert.equal(api.mq_video_upload(id, 1), 0);
    }
    assert.equal(uploads, uploadsBeforeStop);
    api.mq_video_play(id);
    assert.equal(api.mq_video_ready(id), 2, 'replay makes frames presentable again');
    assert.equal(api.mq_video_upload(id, 1), 1, 'replay uploads its first frame even at the same decode count');
    video.ended = true; video.fire('ended'); api.mq_video_pause(id);
    assert.equal(api.mq_video_state(id), 3, 'pause preserves ended state');
    video.ended = false;

    video.reject = { name: 'NotAllowedError', message: 'not allowed' };
    api.mq_video_play(id); await Promise.resolve();
    const length = api.mq_video_error(id, 0, 1024);
    assert.match(Buffer.from(memory.buffer, 0, length).toString(), /Browser blocked video audio/);
    assert.equal(video.paused, true);
    video.reject = null;
    api.mq_video_play(id); await Promise.resolve();
    assert.equal(api.mq_video_error(id, 0, 1024), 0, 'retry clears recoverable autoplay errors');

    // A rejected pending play request must not overwrite a later stop.
    video.reject = new Error('cancelled play');
    api.mq_video_play(id); api.mq_video_stop(id); await Promise.resolve();
    assert.equal(api.mq_video_state(id), 0);
    assert.equal(api.mq_video_error(id, 0, 1024), 0);
    api.mq_video_delete(id);
    assert.equal(video.paused, true); assert.equal(video.src, undefined); assert.equal(video.loads, 2);
    assert.equal(api.mq_video_ready(id), 0);
    console.log('PASS: video registration, bounded sizing, frame deduplication, GL state, playback, visibility, errors and disposal');
}
main().catch(error => { console.error(error); process.exitCode = 1; });
