# Browser canvas configuration

The fork's `js/mq_js_bundle.js` supports optional host settings. Set
`window.macroquadConfig` before `load("app.wasm")` (before or after loading the
bundle). Existing applications can omit it: density still follows `Conf.high_dpi`,
ordinary loops use browser animation frames, and blocking loops wait for updates.
These settings are browser-only; native applications continue to use `Conf`
and the platform's window and presentation behavior.

```html
<script>
window.macroquadConfig = {
    maxDevicePixelRatio: 2,
    maxFramebufferPixels: 3840 * 2160,
    maxFramebufferEdge: 4096,
    targetFps: 30,
};
</script>
<script src="mq_js_bundle.js"></script>
<script>load("app.wasm");</script>
```

Each field is optional. Omitted or `null` fields have no application cap.
Density and frame-rate limits must be positive finite numbers. Pixel and edge
budgets must also be at least one; fractional budgets round down when allocating
integer framebuffer dimensions. Invalid fields are ignored and named in the
report's `rejected` array.

`maxDevicePixelRatio` caps the density selected by `high_dpi`. Pixel and edge
budgets reduce that density further, including below one on large CSS canvases
with `high_dpi: false`. The host uses one scale to keep the aspect ratio, then
rounds framebuffer edges down to integer pixels (minimum one). Logical window
edges can therefore differ from CSS edges by less than one framebuffer pixel.
An extreme aspect ratio and a tiny budget can make aspect preservation
impossible because both edges must be at least one pixel; the host respects
the budget and reports `minimum-framebuffer-edge`.

Sizing also respects WebGL's maximum renderbuffer/viewport dimensions. A browser
that allocates a smaller drawing buffer triggers an aspect-preserving retry;
input, resize notifications and canvas-size imports use the actual drawing
buffer. The report names platform constraints and gives effective dimensions.
GPU limits and observed allocation reductions are conservative context-local
limits, refreshed after context restoration. They do not predict available GPU
memory or guarantee that every allocation succeeds.

The host publishes the effective density **before** calling Rust's resize
handler, including density changes whose rounded framebuffer is unchanged.
Default projection, `screen_width` / `screen_height`, `screen_dpi_scale`,
logical/physical mouse and touch access, and text rasterization use those units.
Current canvas-to-CSS ratios determine input positions. The host checks before
each frame, watches element resizes, and rearms a resolution media query to
detect zoom and monitor-density changes even in an idle blocking loop. Existing
relative-input history is discarded on size or density changes.

`targetFps` is a **maximum requested rate**, not a guaranteed achieved rate.
It enables continuous paced browser frames even with a blocking event loop.
Input and application update requests are coalesced into that same loop and
cannot bypass its limit. Animation callbacks check the elapsed time before
rendering, and a late/background frame does not produce catch-up frames.
Display refresh, browser throttling and rendering cost can reduce the achieved
rate. For example, a refresh rate that is not divisible by the target can
produce fewer frames than requested. Omitting `targetFps` preserves the
ordinary or blocking event-loop choice.

Inspect a snapshot of requested and effective settings:

```js
const report = window.macroquadHost.getReport();
console.log(report.requested);   // Caps, target, highDpi, current DPR and CSS size.
console.log(report.effective);   // Actual framebuffer, density and logical size.
console.log(report.constraints); // Density/pixel/edge or platform reductions.
console.log(report.rejected);    // Invalid configuration field names.
console.log(report.observedFps); // Recent rendered-frame average, or null.
```

`effective.targetFps` is the accepted scheduling cap, while `observedFps` is
the measured rate over up to 120 actual frames. The latter is an observation,
not a prediction; it resets after background suppression or reconfiguration.
Reports are plain snapshots and may be inspected before the application starts;
dimensions become available after initial sizing.

Replace the complete configuration at runtime with
`window.macroquadHost.configure({ targetFps: 15, maxFramebufferEdge: 2048 })`.
It resizes immediately and applies the new pacing to the pending frame.
`configure({})` clears application caps and restores the original event-loop
choice. Applications do not replace global sizing/DPI/scheduler functions or
wrap WASM scheduling imports.

`examples/canvas_host.rs` and `examples/canvas_host.html` form a self-contained
fixture. Build with
`cargo build --target wasm32-unknown-unknown --example canvas_host`, copy the
WASM beside the HTML and serve the repository root over HTTP. The page exposes
`window.canvasHostFixture` and the latest 600 `window.canvasHostFrames` for
checking targets, measured/drawn text bounds, current density and frame cadence.
The native version runs with `cargo run --example canvas_host`.

Run deterministic tests without a browser:

```sh
node js/mq_canvas.test.cjs
node js/mq_input_coordinates.test.cjs
```
