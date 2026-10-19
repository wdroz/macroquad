// Run with: node js/mq_pointer_capture.test.cjs
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const Idle = 0, Requested = 1, Active = 2, Denied = 3, Lost = 4;
const root = path.join(__dirname);
const bundle = fs.readFileSync(path.join(root, 'mq_js_bundle.js'), 'utf8');
const source = fs.readFileSync(path.join(root, 'mq_pointer_capture.js'), 'utf8');
assert(bundle.endsWith(source), 'mq_js_bundle.js must end with js/mq_pointer_capture.js');

function listen(target) {
  const map = new Map();
  target.addEventListener = (type, fn) => {
    if (!map.has(type)) map.set(type, []);
    map.get(type).push(fn);
  };
  target.dispatch = (type, event = {}) => {
    for (const fn of map.get(type) || []) fn(event);
  };
  return target;
}

const requests = [];
let lock = null;
const canvas = listen({
  style: {},
  width: 640,
  height: 480,
  focus() {},
  getBoundingClientRect() { return { left: 0, top: 0, width: 640, height: 480 }; },
  requestPointerLock() {
    requests.push('request');
    const mode = canvas.mode;
    if (mode === 'throw') throw new Error('InvalidStateError');
    if (mode === 'reject') {
      return Promise.reject(Object.assign(new Error('not allowed'), { name: 'NotAllowedError' }));
    }
    if (mode === 'hang') return new Promise(() => {});
    if (mode === 'legacy') return undefined;
    // The element is locked when the promise resolves, as in the browser.
    return Promise.resolve().then(() => { lock = canvas; });
  },
});
canvas.mode = 'grant';
const documentListeners = listen({
  querySelector(selector) { assert.equal(selector, '#glcanvas'); return canvas; },
  hasFocus() { return documentListeners.focused; },
  visibilityState: 'visible',
  focused: true,
  get pointerLockElement() { return lock; },
  set pointerLockElement(value) { lock = value; },
  exitPointerLock() {
    requests.push('exit');
    lock = null;
  },
});
const windowListeners = listen({ devicePixelRatio: 1 });
const context = vm.createContext({
  console,
  Promise,
  document: documentListeners,
  window: windowListeners,
});
vm.runInContext(bundle, context, { filename: 'mq_js_bundle.js' });
context.register_plugins(context.plugins);
const env = context.importObject.env;
assert.equal(env.mq_capture_ready(), 1);
assert.equal(env.mq_capture_state(), Idle);

async function flush() {
  await new Promise(resolve => setImmediate(resolve));
}

(async () => {
  env.mq_capture_set_wanted(1);
  env.mq_capture_set_wanted(1);
  canvas.requestPointerLock();
  assert.equal(requests.filter(kind => kind === 'request').length, 1);
  assert.equal(env.mq_capture_state(), Requested);
  await flush();
  assert.equal(env.mq_capture_state(), Active);

  documentListeners.dispatch('pointerlockchange');
  assert.equal(env.mq_capture_state(), Active);
  lock = null;
  documentListeners.dispatch('pointerlockchange');
  documentListeners.dispatch('pointerlockchange');
  documentListeners.dispatch('pointerlockchange');
  assert.equal(env.mq_capture_state(), Lost);
  assert.equal(requests.filter(kind => kind === 'request').length, 1);

  for (let i = 0; i < 8; i++) env.mq_capture_set_wanted(1);
  canvas.requestPointerLock();
  assert.equal(requests.filter(kind => kind === 'request').length, 1, 'lost capture does not retry every call');

  windowListeners.dispatch('pointerdown', { type: 'pointerdown' });
  assert.equal(requests.filter(kind => kind === 'request').length, 2);
  await flush();
  assert.equal(env.mq_capture_state(), Active);

  canvas.mode = 'reject';
  lock = null;
  documentListeners.dispatch('pointerlockchange');
  assert.equal(env.mq_capture_state(), Lost);
  windowListeners.dispatch('mousedown', { type: 'mousedown' });
  await flush();
  assert.equal(env.mq_capture_state(), Denied);
  const deniedRequests = requests.filter(kind => kind === 'request').length;
  for (let i = 0; i < 5; i++) env.mq_capture_set_wanted(1);
  assert.equal(requests.filter(kind => kind === 'request').length, deniedRequests);

  canvas.mode = 'grant';
  windowListeners.dispatch('touchstart', { type: 'touchstart' });
  await flush();
  assert.equal(env.mq_capture_state(), Active);

  documentListeners.focused = false;
  windowListeners.dispatch('blur');
  windowListeners.dispatch('focus');
  assert.equal(env.mq_capture_state(), Lost, 'focus returning does not recapture');
  assert.equal(lock, null);
  documentListeners.focused = true;
  windowListeners.dispatch('focus');
  assert.equal(env.mq_capture_state(), Lost);
  windowListeners.dispatch('pointerdown', { type: 'pointerdown' });
  await flush();
  assert.equal(env.mq_capture_state(), Active);

  const beforeEscape = requests.filter(kind => kind === 'request').length;
  windowListeners.dispatch('keydown', { type: 'keydown', code: 'Escape', key: 'Escape' });
  lock = null;
  documentListeners.dispatch('pointerlockchange');
  windowListeners.dispatch('keydown', { type: 'keydown', code: 'Escape', key: 'Escape', repeat: true });
  documentListeners.dispatch('pointerlockchange');
  assert.equal(env.mq_capture_state(), Lost);
  assert.equal(requests.filter(kind => kind === 'request').length, beforeEscape);

  env.mq_capture_set_wanted(0);
  assert.equal(env.mq_capture_state(), Idle);
  windowListeners.dispatch('pointerdown', { type: 'pointerdown' });
  assert.equal(env.mq_capture_state(), Idle, 'a release cancels the gesture retry');

  canvas.mode = 'throw';
  env.mq_capture_set_wanted(1);
  assert.equal(env.mq_capture_state(), Denied);
  canvas.mode = 'grant';
  windowListeners.dispatch('pointerdown', { type: 'pointerdown' });
  await flush();
  assert.equal(env.mq_capture_state(), Active);

  canvas.mode = 'hang';
  lock = null;
  documentListeners.dispatch('pointerlockchange');
  assert.equal(env.mq_capture_state(), Lost);
  windowListeners.dispatch('pointerdown', { type: 'pointerdown' });
  windowListeners.dispatch('mousedown', { type: 'mousedown' });
  assert.equal(env.mq_capture_state(), Requested);
  const hung = requests.filter(kind => kind === 'request').length;
  env.mq_capture_set_wanted(1);
  assert.equal(requests.filter(kind => kind === 'request').length, hung, 'pending request is not duplicated');

  env.mq_capture_set_wanted(0);
  assert.equal(env.mq_capture_state(), Idle);
  canvas.mode = 'legacy';
  env.mq_capture_set_wanted(1);
  assert.equal(env.mq_capture_state(), Requested);
  lock = canvas;
  documentListeners.dispatch('pointerlockchange');
  assert.equal(env.mq_capture_state(), Active);
  documentListeners.dispatch('pointerlockerror');
  assert.equal(env.mq_capture_state(), Active, 'an error while locked does not drop capture');
  lock = null;
  documentListeners.dispatch('pointerlockerror');
  assert.equal(env.mq_capture_state(), Denied);

  console.log('PASS: pointer capture dedupe, denial, escape, focus loss, and gesture recovery');
})().catch(error => {
  console.error(error);
  process.exit(1);
});
