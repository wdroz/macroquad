// Run with: node js/mq_canvas.test.cjs
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { createHost } = require('./tests/support/canvas-host.cjs');

const helper = fs.readFileSync(path.join(__dirname, 'mq_canvas.js'), 'utf8');
assert(fs.readFileSync(path.join(__dirname, 'mq_js_bundle.js'), 'utf8').includes(helper),
    'bundle must include the unmodified host helper');

function near(actual, expected, tolerance = 1e-8) {
    assert(Math.abs(actual - expected) <= tolerance, `${actual} != ${expected}`);
}

// Defaults: follow DPR only when requested, draw at each RAF, preserve idle blocking.
for (const highDpi of [false, true]) {
    const host = createHost(undefined, { dpr: 3, highDpi });
    assert.equal(host.report().effective.density, highDpi ? 3 : 1);
    for (const t of [0, 7, 16, 24]) host.step(t);
    assert.deepEqual(host.frames, [0, 7, 16, 24]);
    assert.equal(host.report().requested.targetFps, null);
}
const idle = createHost(undefined, { blocking: true });
idle.step(0);
assert.equal(idle.pending(), 0);
for (let i = 0; i < 50; i++) idle.request();
assert.equal(idle.pending(), 1);
idle.step(5);
assert.deepEqual(idle.frames, [0, 5]);
assert.equal(idle.pending(), 0);
idle.density(2); // Detect idle monitor changes without relying on a window resize.
assert.deepEqual(idle.resizes.at(-1), [1600, 1200, 2]);
assert.equal(idle.pending(), 1);

// Density, pixel and edge caps apply isotropically, even when high_dpi=false.
for (const highDpi of [false, true]) {
    const host = createHost({ maxDevicePixelRatio: 2, maxFramebufferPixels: 1920 * 1080,
        maxFramebufferEdge: 1600 }, { width: 3840, height: 2160, dpr: 3, highDpi });
    const r = host.report();
    assert.equal(r.requested.maxDevicePixelRatio, 2);
    assert.equal(r.effective.framebufferWidth, 1600);
    assert.equal(r.effective.framebufferHeight, 900);
    near(r.effective.density, 1600 / 3840);
    near(r.effective.logicalWidth, 3840);
    near(r.effective.logicalHeight, 2160);
    assert(r.constraints.includes('pixel-budget'));
    assert(r.constraints.includes('edge-budget'));
    host.move(500, 200);
    host.touchAt(500, 200);
    assert.deepEqual(host.mouse.at(-1), [Math.floor(500 * r.effective.density), Math.floor(200 * r.effective.density)]);
    near(host.touch.at(-1)[0] / r.effective.density, 500);
    near(host.touch.at(-1)[1] / r.effective.density, 200);
}
const pixel = createHost({ maxFramebufferPixels: 120000 }, { dpr: 2 });
assert.deepEqual([pixel.canvas.width, pixel.canvas.height], [400, 300]);
pixel.resize(1600, 900);
assert(pixel.canvas.width * pixel.canvas.height <= 120000);
near(pixel.canvas.width / pixel.canvas.height, 1600 / 900, 0.01);

// Subpixel/fractional dimensions round by less than one physical pixel.
const fractional = createHost({ maxDevicePixelRatio: 1.25 }, { width: 801.25, height: 602.5, dpr: 2 });
const fr = fractional.report().effective;
near(fr.logicalWidth, 801.25, 1 / fr.density);
near(fr.logicalHeight, 602.5, 1 / fr.density);
for (const [width, height] of [[10000, 1], [1, 10000]]) {
    const tiny = createHost({ maxFramebufferPixels: 1 }, { width, height });
    assert.equal(tiny.canvas.width * tiny.canvas.height, 1);
    assert(tiny.report().constraints.includes('minimum-framebuffer-edge'));
}
const invalid = createHost({ maxDevicePixelRatio: -1, maxFramebufferPixels: 0.5, maxFramebufferEdge: NaN, targetFps: Infinity },
    { width: 0, height: NaN, dpr: NaN });
assert.deepEqual([invalid.canvas.width, invalid.canvas.height], [1, 1]);
assert.equal(invalid.report().rejected.length, 4);

// GPU limits and a browser allocating a smaller buffer are observable and synchronized.
const gpu = createHost({ maxFramebufferEdge: 4096 }, { width: 2000, height: 1000, dpr: 2,
    platform: { maxEdge: 2048, maxWidth: 1600 } });
assert.deepEqual([gpu.report().effective.framebufferWidth, gpu.report().effective.framebufferHeight, gpu.report().effective.density], [1600, 800, 0.8]);
assert.equal(gpu.report().requested.maxFramebufferEdge, 4096);
assert(gpu.report().constraints.includes('platform-width'));
const reduced = createHost({}, { width: 2000, height: 1000, dpr: 2,
    platform: { allocationWidth: 1200, allocationHeight: 900 } });
assert.deepEqual([reduced.canvas.width, reduced.canvas.height], [1200, 600]);
assert.equal(reduced.report().effective.density, 0.6);
assert(reduced.report().constraints.includes('platform-drawing-buffer'));
assert.equal(reduced.context.importObject.env.canvas_width(), 1200);
reduced.step(0);
assert.deepEqual([reduced.canvas.width, reduced.canvas.height], [1200, 600]);
const reorient = createHost({}, { width: 10000, height: 500, highDpi: false,
    platform: { allocationWidth: 4096, allocationHeight: 4096 } });
reorient.resize(500, 10000);
assert.deepEqual([reorient.canvas.width, reorient.canvas.height], [204, 4096]);

// Density-only changes notify Rust even if the rounded framebuffer stays fixed.
const density = createHost({}, { width: 10, height: 10, dpr: 1.01, blocking: true });
density.step(0);
density.move(0, 0, 0.75, 0);
const before = density.resizes.length;
density.density(1.02);
assert.equal(density.resizes.length, before + 1);
assert.deepEqual(density.resizes.at(-1), [10, 10, 1.02]);
density.move(0, 0, 0.3, 0);
assert.deepEqual(density.raw, [], 'density change clears raw subpixel motion history');

// Frame requests from input and resize never bypass the cap. No catch-up bursts.
const paced = createHost({ targetFps: 30 }, { blocking: true });
for (let t = 0; t <= 1000; t += 5) {
    for (let n = 0; n < 10; n++) paced.request();
    assert.equal(paced.pending(), 1);
    paced.step(t);
}
assert(paced.frames.length >= 28 && paced.frames.length <= 31);
assert(paced.report().observedFps <= 30.000001);
for (let i = 1; i < paced.frames.length; i++) assert(paced.frames[i] - paced.frames[i - 1] >= 1000 / 30 - 1e-6);
paced.step(10000);
paced.step(10001);
assert.equal(paced.frames.at(-1), 10000);
paced.document.visibilityState = 'hidden';
const frames = paced.frames.length;
paced.step(20000);
assert.equal(paced.frames.length, frames);
paced.document.visibilityState = 'visible';
paced.step(20001);
assert.equal(paced.frames.at(-1), 20001);
assert.equal(paced.report().observedFps, null);

// A main-thread stall can make RAF's timestamp much older than wall time.
// The next current-timestamp callback must not repay it with an immediate frame.
const stalled = createHost({ targetFps: 30 }, { blocking: true });
stalled.step(0);
stalled.step(100, 170);
stalled.step(170, 170.2);
assert.deepEqual(stalled.frames, [0, 170]);
stalled.step(180, 203.4);
assert.deepEqual(stalled.frames, [0, 170, 203.4]);

// Runtime updates replace the complete config and affect an already queued RAF.
paced.configure({ targetFps: 10, maxDevicePixelRatio: 0.5 });
paced.step(20002);
paced.step(20050);
assert.equal(paced.frames.at(-1), 20002);
assert.equal(paced.report().effective.density, 0.5);
paced.configure({});
assert.equal(paced.report().requested.targetFps, null);
paced.step(20100);
assert.equal(paced.pending(), 0, 'removing the target returns to idle blocking behavior');

console.log('PASS: default RAF/blocking, density/pixel/edge/GPU budgets, actual framebuffer, resize/DPR/input synchronization, deterministic pacing, reporting');
