// A deterministic browser boundary for the real, complete loader bundle.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function listen(target) {
    const listeners = new Map();
    target.addEventListener = (type, callback) => {
        if (!listeners.has(type)) listeners.set(type, new Set());
        listeners.get(type).add(callback);
    };
    target.removeEventListener = (type, callback) => listeners.get(type)?.delete(callback);
    target.dispatch = (type, event = {}) => {
        for (const callback of [...(listeners.get(type) || [])]) callback(event);
    };
    return target;
}

function createHost(config, options = {}) {
    let now = 0, nextId = 0;
    const queued = new Map(), media = [];
    const rect = { left: 0, top: 0, width: options.width ?? 800, height: options.height ?? 600 };
    const canvas = listen({
        style: {}, width: 800, height: 600, clientWidth: rect.width, clientHeight: rect.height,
        focus() {}, getBoundingClientRect() { return rect; },
    });
    const document = listen({
        visibilityState: 'visible', pointerLockElement: null,
        querySelector() { return canvas; }, hasFocus() { return true; },
    });
    const window = listen({
        devicePixelRatio: options.dpr ?? 1, macroquadConfig: config,
        requestAnimationFrame(callback) { const id = ++nextId; queued.set(id, callback); return id; },
        cancelAnimationFrame(id) { queued.delete(id); },
        matchMedia() { const query = listen({}); media.push(query); return query; },
        ResizeObserver: class {
            constructor(callback) { this.callback = callback; }
            observe() { window.resizeObserver = this; }
        },
    });
    const context = vm.createContext({ console, TextEncoder, TextDecoder, document, window,
        performance: { now: () => now } });
    const bundle = fs.readFileSync(path.join(__dirname, '../../mq_js_bundle.js'), 'utf8');
    vm.runInContext(bundle, context, { filename: 'mq_js_bundle.js' });
    context.register_plugins(context.plugins);
    const frames = [], resizes = [], mouse = [], touch = [], raw = [];
    context.wasm_exports = {
        frame() { frames.push(now); options.onFrame?.(host); },
        resize(width, height) { resizes.push([width, height, context.importObject.env.dpi_scale()]); },
        mouse_move(x, y) { mouse.push([x, y]); }, raw_mouse_move(x, y) { raw.push([x, y]); },
        touch(_phase, _id, x, y) { touch.push([x, y]); },
    };
    if (options.platform) {
        context.gl = {
            MAX_RENDERBUFFER_SIZE: 1, MAX_VIEWPORT_DIMS: 2,
            getParameter(name) { return name === 1 ? options.platform.maxEdge ?? 16384 :
                [options.platform.maxWidth ?? 16384, options.platform.maxHeight ?? 16384]; },
            get drawingBufferWidth() { return Math.min(canvas.width, options.platform.allocationWidth ?? Infinity); },
            get drawingBufferHeight() { return Math.min(canvas.height, options.platform.allocationHeight ?? Infinity); },
        };
    }
    const host = {
        context, window, document, canvas, rect, frames, resizes, mouse, touch, raw,
        report: () => window.macroquadHost.getReport(),
        configure: config => window.macroquadHost.configure(config),
        pending: () => queued.size,
        request: () => context.importObject.env.sapp_schedule_update(),
        step(timestamp, actualTime = timestamp) {
            now = actualTime;
            const callbacks = [...queued.values()];
            queued.clear();
            for (const callback of callbacks) callback(timestamp);
        },
        resize(width, height) {
            rect.width = canvas.clientWidth = width;
            rect.height = canvas.clientHeight = height;
            window.onresize();
        },
        density(dpr) {
            window.devicePixelRatio = dpr;
            for (const query of [...media]) query.dispatch('change');
        },
        move(x, y, dx = 0, dy = 0) {
            canvas.onmousemove({ clientX: rect.left + x, clientY: rect.top + y, movementX: dx, movementY: dy });
        },
        touchAt(x, y) {
            canvas.dispatch('touchstart', { preventDefault() {}, changedTouches: [{ identifier: 1, clientX: rect.left + x, clientY: rect.top + y }] });
        },
    };
    context.importObject.env.setup_canvas_size(options.highDpi ?? true);
    context.importObject.env.run_animation_loop(options.blocking ?? false);
    return host;
}

module.exports = { createHost };
