// Run with: node js/mq_input_coordinates.test.cjs
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function listen(target) {
    const listeners = new Map();
    target.addEventListener = (type, callback) => {
        if (!listeners.has(type)) listeners.set(type, []);
        listeners.get(type).push(callback);
    };
    target.dispatch = (type, event = {}) => {
        for (const callback of listeners.get(type) || []) callback(event);
    };
    return target;
}

const rect = { left: 20, top: 30, width: 800, height: 600 };
const canvas = listen({
    style: {}, width: 800, height: 600, clientWidth: 800, clientHeight: 600,
    focus() {}, getBoundingClientRect() { return rect; },
});
const document = listen({
    visibilityState: 'visible', pointerLockElement: null,
    querySelector() { return canvas; }, hasFocus() { return true; },
});
const window = listen({ devicePixelRatio: 1, requestAnimationFrame() { return 1; } });
const context = vm.createContext({ console, TextEncoder, TextDecoder, document, window });
const bundle = fs.readFileSync(path.join(__dirname, 'mq_js_bundle.js'), 'utf8');
const helper = fs.readFileSync(path.join(__dirname, 'mq_input_coordinates.js'), 'utf8');
assert(bundle.includes(helper), 'bundle must include the coordinate helper source');
vm.runInContext(bundle, context);
context.register_plugins(context.plugins);
context.high_dpi = true;
const mouse = [], touch = [], raw = [];
context.wasm_exports = {
    mouse_move: (x, y) => mouse.push([x, y]),
    raw_mouse_move: (x, y) => raw.push([x, y]),
    touch: (phase, id, x, y) => touch.push([x, y]),
};
context.importObject.env.run_animation_loop(false);

function move(x, y, dx = 0, dy = 0) {
    canvas.onmousemove({ clientX: rect.left + x, clientY: rect.top + y, movementX: dx, movementY: dy });
}
for (const density of [1, 2]) {
    window.devicePixelRatio = density;
    canvas.width = rect.width * density;
    canvas.height = rect.height * density;
    move(160, 120);
    canvas.dispatch('touchstart', {
        preventDefault() {}, changedTouches: [{ identifier: 1, clientX: rect.left + 160, clientY: rect.top + 120 }],
    });
    assert.deepEqual(mouse.at(-1), [160 * density, 120 * density]);
    assert.deepEqual(touch.at(-1), mouse.at(-1), `mouse and touch match at ${density}x`);
    assert.equal(context.importObject.env.dpi_scale(), density);
}

// Forward actual framebuffer/element ratios, including bounded or CSS-scaled canvases.
canvas.width = 960;
canvas.height = 900;
move(160, 120);
assert.deepEqual(mouse.at(-1), [192, 180]);
raw.length = 0;
move(0, 0, 5, -2);
assert.deepEqual(raw, [[6, -3]]);

// Preserve fractional browser motion through miniquad's integer WASM ABI.
canvas.width = 1600;
canvas.height = 1200;
raw.length = 0;
for (let i = 0; i < 8; i++) move(0, 0, 0.25, -0.125);
assert.equal(raw.reduce((sum, [x]) => sum + x, 0), 4);
assert.equal(raw.reduce((sum, [, y]) => sum + y, 0), -2);
assert(raw.every(([x, y]) => Number.isInteger(x) && Number.isInteger(y)));

function residualReset(change) {
    window.devicePixelRatio = 1;
    canvas.width = 800;
    canvas.height = 600;
    document.dispatch('pointerlockchange');
    raw.length = 0;
    move(0, 0, 0.75, 0);
    assert.deepEqual(raw, []);
    change();
    move(0, 0, 0.125, 0);
    assert.deepEqual(raw, [], 'geometry/density/capture change discards the old subpixel remainder');
}
residualReset(() => { canvas.width = 1600; canvas.height = 1200; });
residualReset(() => { window.devicePixelRatio = 2; }); // No framebuffer resize.
residualReset(() => { rect.width = 400; rect.height = 300; });
rect.width = 800; rect.height = 600;
residualReset(() => { document.pointerLockElement = canvas; document.dispatch('pointerlockchange'); });

const before = raw.length;
move(0, 0, NaN, Infinity);
assert.equal(raw.length, before, 'invalid raw motion is not forwarded');
console.log('PASS: mouse/touch targets at 1x/2x, current density, framebuffer ratios, fractional motion, resize/DPI/capture resets');
