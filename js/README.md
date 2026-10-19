mq_js_bundle.js contains most common macroquad's dependencies JS code in one JS file.

`mq_js_bundle.js` was made by:

```bash
#!/bin/bash

function wrap_js {
    echo "(function () {" >> mq_js_bundle.js
    echo "var register_plugin;" >> mq_js_bundle.js
    cat $1 >> mq_js_bundle.js
    echo "}());" >> mq_js_bundle.js
}
cat ../../miniquad/js/gl.js > mq_js_bundle.js
wrap_js ../../quad-snd/js/audio.js
wrap_js ../../sapp-jsutils/js/sapp_jsutils.js
wrap_js ../../quad-net/js/quad-net.js 
minify mq_js_bundle.js > mq_js_bundle2.js
```

The `quad_net` wrapper declares its `register_plugin` variable locally because
the miniquad loader enables strict mode for the entire bundle. Preserve that
declaration when regenerating the bundle. Run its registration regression test
without a browser or graphics context:

```sh
node js/mq_js_bundle.test.cjs
```

The fork also requires `glDepthMask` to forward to `gl.depthMask(Boolean(value))`
so transparent meshes can depth-test without writing depth. Preserve that
binding when regenerating the bundle. The test checks both disabling and
enabling writes. Append `mq_pointer_capture.js` at the end of the bundle when
regenerating it. That plugin is the browser half of `set_cursor_grab`: it
deduplicates Pointer Lock requests, records denial and Escape or focus loss,
and does not request again until a pointer or touch press. `node js/mq_pointer_capture.test.cjs`
checks that sequence and that the bundle still ends with the plugin source. Pass a final WASM build to verify that every imported GL
function has a real binding, before the loader can replace omissions with
logging stubs:

```sh
node js/mq_js_bundle.test.cjs path/to/game.wasm
```

Include `mq_canvas.js`, then `mq_input_coordinates.js`, immediately before the pointer-capture source.
The loader's `resize`, `dpi_scale`, `animation`, `canvas_width`/`canvas_height`,
`setup_canvas_size`, `run_animation_loop` and `sapp_schedule_update` bindings
delegate directly to the canvas helper. Preserve those delegations when
regenerating the miniquad portion; the helper is built in, not a plugin that
overrides globals or imports. [Browser host configuration](../docs/browser-host.md)
documents optional density/framebuffer budgets and pacing, runtime updates,
requested/effective reports, rounding and browser limitations. Check it with:

```sh
node js/mq_canvas.test.cjs
```

When regenerating the miniquad portion, keep `mouse_relative_position`
delegating to `macroquad_input_position`, and forward browser movement through
`macroquad_input_raw_motion` instead of flooring each event. The helper maps
mouse and touch positions through the actual canvas-to-CSS ratios and preserves
fractional relative motion across miniquad's integer WASM ABI. It discards
fractional remainders when canvas geometry, display density or pointer lock
changes. It does not alter canvas sizing or frame scheduling. Run:

```sh
node js/mq_input_coordinates.test.cjs
```

The optional `video` feature uses the separate `mq_video.js` plugin. Load it
after the bundle and before `load("app.wasm")`. See `docs/video.md` for the
playback API and native decoder requirements. Check its browser lifecycle and
GL-state behavior with `node js/mq_video.test.cjs`.
