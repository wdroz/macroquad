// macroquad browser host configuration 1
// Included in mq_js_bundle.js. The loader calls these functions directly;
// applications do not replace resize, dpi_scale, animation or WASM imports.
const macroquad_canvas_host = {
    requested: null, options: {}, rejected: [], density: null, report: null,
    started: false, pending: false, running: false, raf: null,
    lastFrame: null, frameTimes: [], media: null, observer: null, gl: null, limits: {},
};

function macroquad_canvas_configure(options) {
    const host = macroquad_canvas_host;
    host.requested = {};
    host.report = null;
    host.options = {};
    host.rejected = [];
    for (const name of ["maxDevicePixelRatio", "maxFramebufferPixels", "maxFramebufferEdge", "targetFps"]) {
        const value = options?.[name];
        host.requested[name] = value ?? null;
        if (value == null) continue;
        if (typeof value === "number" && Number.isFinite(value) && value > 0 &&
            (name === "targetFps" || name === "maxDevicePixelRatio" || value >= 1)) {
            host.options[name] = value;
        } else {
            host.rejected.push(name);
        }
    }
    host.lastFrame = null;
    host.frameTimes = [];
    if (host.started) {
        macroquad_canvas_resize(canvas, wasm_exports.resize);
        macroquad_canvas_request();
    }
    return macroquad_canvas_report();
}

function macroquad_canvas_options() {
    if (macroquad_canvas_host.requested === null) {
        macroquad_canvas_configure(window.macroquadConfig);
    }
    return macroquad_canvas_host.options;
}

function macroquad_canvas_device_density() {
    const value = window.devicePixelRatio;
    return Number.isFinite(value) && value > 0 ? value : 1;
}

function macroquad_canvas_density() {
    // Before initial sizing, preserve miniquad's density query semantics.
    return macroquad_canvas_host.density ?? (high_dpi ? macroquad_canvas_device_density() : 1);
}

function macroquad_canvas_size(cssWidth, cssHeight, deviceDensity, highDpi, options, limits = {}) {
    const width = Number.isFinite(cssWidth) && cssWidth > 0 ? cssWidth : 1;
    const height = Number.isFinite(cssHeight) && cssHeight > 0 ? cssHeight : 1;
    const requestedDensity = highDpi ? deviceDensity : 1;
    const constraints = [];
    let density = requestedDensity;
    function cap(value, reason) {
        if (Number.isFinite(value) && value > 0 && value < density) {
            density = value;
            constraints.push(reason);
        }
    }
    cap(options.maxDevicePixelRatio, "density-cap");
    cap(Math.sqrt(options.maxFramebufferPixels / width / height), "pixel-budget");
    cap(options.maxFramebufferEdge / Math.max(width, height), "edge-budget");
    cap(limits.width / width, "platform-width");
    cap(limits.height / height, "platform-height");
    let physicalWidth = Math.max(1, Math.floor(width * density));
    let physicalHeight = Math.max(1, Math.floor(height * density));
    // A canvas edge cannot be smaller than one pixel. At extreme aspect ratios
    // that minimum can exceed a pixel budget even after isotropic scaling.
    if (options.maxFramebufferPixels && physicalWidth * physicalHeight > options.maxFramebufferPixels) {
        if (physicalWidth >= physicalHeight) physicalWidth = Math.max(1, Math.floor(options.maxFramebufferPixels / physicalHeight));
        else physicalHeight = Math.max(1, Math.floor(options.maxFramebufferPixels / physicalWidth));
        density = Math.min(physicalWidth / width, physicalHeight / height);
        constraints.push("minimum-framebuffer-edge");
    }
    return {
        cssWidth: width, cssHeight: height, requestedDensity, density,
        width: physicalWidth, height: physicalHeight, constraints,
    };
}

function macroquad_canvas_limits() {
    if (typeof gl === "undefined" || !gl?.getParameter) return {};
    const host = macroquad_canvas_host;
    if (host.gl === gl) return host.limits;
    const edge = gl.getParameter(gl.MAX_RENDERBUFFER_SIZE);
    const viewport = gl.getParameter(gl.MAX_VIEWPORT_DIMS);
    host.gl = gl;
    host.limits = {
        width: Math.min(edge || Infinity, viewport?.[0] || Infinity),
        height: Math.min(edge || Infinity, viewport?.[1] || Infinity),
    };
    return host.limits;
}

function macroquad_canvas_width(element) {
    return typeof gl !== "undefined" && gl?.drawingBufferWidth > 0 ? gl.drawingBufferWidth : element.width;
}

function macroquad_canvas_height(element) {
    return typeof gl !== "undefined" && gl?.drawingBufferHeight > 0 ? gl.drawingBufferHeight : element.height;
}

function macroquad_canvas_resize(element, callback) {
    const host = macroquad_canvas_host;
    const options = macroquad_canvas_options();
    const size = macroquad_canvas_size(element.clientWidth, element.clientHeight,
        macroquad_canvas_device_density(), high_dpi, options, macroquad_canvas_limits());
    const previousWidth = macroquad_canvas_width(element);
    const previousHeight = macroquad_canvas_height(element);
    const previousDensity = host.density;
    // Publish density BEFORE invoking Rust's resize callback. Its input history,
    // projection and font rasterization must all observe the same new units.
    host.density = size.density;
    if (element.width !== size.width) element.width = size.width;
    if (element.height !== size.height) element.height = size.height;
    for (let attempt = 0; attempt < 2; attempt++) {
        const actualWidth = macroquad_canvas_width(element);
        const actualHeight = macroquad_canvas_height(element);
        if (actualWidth === element.width && actualHeight === element.height) break;
        if (!size.constraints.includes("platform-drawing-buffer")) size.constraints.push("platform-drawing-buffer");
        // Only remember axes the browser actually reduced, not the other edge
        // of our aspect-preserving retry (which may grow after a CSS resize).
        if (actualWidth < element.width) host.limits.width = Math.min(host.limits.width ?? Infinity, actualWidth);
        if (actualHeight < element.height) host.limits.height = Math.min(host.limits.height ?? Infinity, actualHeight);
        const ratio = Math.min(actualWidth / element.width, actualHeight / element.height);
        if (ratio >= 1) break;
        size.density *= ratio;
        element.width = Math.max(1, Math.floor(size.cssWidth * size.density));
        element.height = Math.max(1, Math.floor(size.cssHeight * size.density));
    }
    const drawingWidth = macroquad_canvas_width(element);
    const drawingHeight = macroquad_canvas_height(element);
    host.density = size.density;
    host.report = {
        requested: { ...host.requested, highDpi: high_dpi, devicePixelRatio: macroquad_canvas_device_density(),
            cssWidth: size.cssWidth, cssHeight: size.cssHeight },
        effective: {
            ...host.options, density: size.density, framebufferWidth: drawingWidth, framebufferHeight: drawingHeight,
            logicalWidth: drawingWidth / size.density, logicalHeight: drawingHeight / size.density,
            targetFps: options.targetFps ?? null,
        },
        constraints: size.constraints, rejected: [...host.rejected],
    };
    if (callback && (previousWidth !== drawingWidth || previousHeight !== drawingHeight || previousDensity !== size.density)) {
        callback(drawingWidth, drawingHeight);
    }
}

function macroquad_canvas_report() {
    const host = macroquad_canvas_host;
    const times = host.frameTimes;
    const observedFps = times.length > 1 ? (times.length - 1) * 1000 / (times.at(-1) - times[0]) : null;
    return {
        ...(host.report ?? { requested: { ...host.requested }, effective: { ...host.options }, constraints: [], rejected: [...host.rejected] }),
        observedFps,
        pacing: "targetFps is an upper bound; display refresh, browser throttling and rendering cost may reduce the achieved rate",
    };
}

function macroquad_canvas_request() {
    const host = macroquad_canvas_host;
    host.pending = true;
    if (host.started && host.raf === null && !host.running) {
        host.raf = window.requestAnimationFrame(animation);
        animation_frame_timeout = host.raf;
    }
}

function macroquad_canvas_frame(timestamp) {
    const host = macroquad_canvas_host;
    const options = macroquad_canvas_options();
    host.raf = null;
    animation_frame_timeout = null;
    host.running = true;
    macroquad_canvas_resize(canvas, wasm_exports.resize);
    // RAF timestamps can be stale after a busy main thread. Admission must use
    // the wall clock at the render boundary or the next RAF can immediately
    // render a second frame to repay that stale timestamp.
    const now = options.targetFps || !Number.isFinite(timestamp) ? performance.now() : timestamp;
    const hidden = options.targetFps && document.visibilityState === "hidden";
    if (hidden) {
        // Resuming must not replay any frames missed in a background tab.
        host.lastFrame = null;
        host.frameTimes = [];
    } else if (!options.targetFps || host.lastFrame === null ||
        now - host.lastFrame + 0.000001 >= 1000 / options.targetFps) {
        host.pending = false;
        host.lastFrame = now;
        host.frameTimes.push(now);
        if (host.frameTimes.length > 120) host.frameTimes.shift();
        wasm_exports.frame();
    }
    host.running = false;
    if (!window.blocking_event_loop || options.targetFps || host.pending) macroquad_canvas_request();
}

function macroquad_canvas_watch_density() {
    const host = macroquad_canvas_host;
    if (!window.matchMedia) return;
    if (host.media) host.media.removeEventListener("change", macroquad_canvas_density_changed);
    host.media = window.matchMedia(`(resolution: ${macroquad_canvas_device_density()}dppx)`);
    host.media.addEventListener("change", macroquad_canvas_density_changed);
}

function macroquad_canvas_density_changed() {
    macroquad_canvas_watch_density();
    macroquad_canvas_resize(canvas, wasm_exports.resize);
    macroquad_canvas_request();
}

function macroquad_canvas_start() {
    macroquad_canvas_host.started = true;
    canvas.addEventListener("webglcontextrestored", () => {
        macroquad_canvas_host.gl = null;
        macroquad_canvas_density_changed();
    });
    macroquad_canvas_watch_density();
    if (window.ResizeObserver) {
        macroquad_canvas_host.observer = new window.ResizeObserver(() => {
            macroquad_canvas_resize(canvas, wasm_exports.resize);
            macroquad_canvas_request();
        });
        macroquad_canvas_host.observer.observe(canvas);
    }
    macroquad_canvas_request();
}

// Configure before load(), or replace the complete configuration at runtime.
// Reports are snapshots; mutating one does not alter the host configuration.
window.macroquadHost = {
    configure: macroquad_canvas_configure,
    getReport() {
        macroquad_canvas_options();
        return JSON.parse(JSON.stringify(macroquad_canvas_report()));
    },
};
