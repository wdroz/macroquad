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
console.log('PASS: strict-mode bundle evaluation and all three plugin registrations');
