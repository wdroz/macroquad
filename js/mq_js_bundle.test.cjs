// Run with: node js/mq_js_bundle.test.cjs
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const canvas = {
  style: {},
  width: 640,
  height: 480,
  focus() {},
  addEventListener() {},
  getBoundingClientRect() { return { left: 0, top: 0, width: 640, height: 480 }; },
};
const context = vm.createContext({
  console,
  TextDecoder,
  TextEncoder,
  document: {
    querySelector(selector) { assert.equal(selector, '#glcanvas'); return canvas; },
    addEventListener() {},
  },
  window: { addEventListener() {}, devicePixelRatio: 1 },
});
const bundle = fs.readFileSync(path.join(__dirname, 'mq_js_bundle.js'), 'utf8');
vm.runInContext(bundle, context, { filename: 'mq_js_bundle.js' });
const names = Array.from(context.plugins, plugin => plugin.name);
assert.deepEqual(names, ['macroquad_audio', 'sapp_jsutils', 'quad_net']);
assert.equal(Object.hasOwn(context, 'register_plugin'), false, 'quad_net registration must stay local');
context.register_plugins(context.plugins);
for (const name of ['audio_init', 'js_create_object', 'ws_connect', 'ws_is_connected', 'ws_send', 'ws_try_recv', 'http_make_request', 'http_try_recv']) {
  assert.equal(typeof context.importObject.env[name], 'function', `${name} is registered`);
}
assert.equal(context.importObject.env.ws_is_connected(), 0);

// Transparent 3D drawing disables only depth writes, then restores the mask.
// Missing GL imports become noisy runtime stubs, so exercise this binding here.
const depthMasks = [];
context.gl = { depthMask: value => depthMasks.push(value) };
context.importObject.env.glDepthMask(0);
context.importObject.env.glDepthMask(1);
assert.deepEqual(depthMasks, [false, true]);
console.log('PASS: strict-mode bundle, plugin registrations, and WebGL depth-mask forwarding');

// Optionally audit an actual final build without instantiating it or opening a
// browser. Keep this before add_missing_functions_stabs can hide omissions.
if (process.argv[2]) {
  const module = new WebAssembly.Module(fs.readFileSync(process.argv[2]));
  const glImports = WebAssembly.Module.imports(module).filter(
    item => item.module === 'env' && item.kind === 'function' && item.name.startsWith('gl'),
  );
  assert(glImports.length > 0, 'Expected a macroquad WASM module with GL imports');
  const missing = glImports.filter(item => typeof context.importObject.env[item.name] !== 'function');
  assert.deepEqual(missing, [], `Missing WebGL bindings: ${missing.map(item => item.name).join(', ')}`);
  console.log(`PASS: all ${glImports.length} GL imports in ${process.argv[2]} have real bundle bindings`);
}
