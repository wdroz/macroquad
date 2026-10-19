;
// macroquad pointer capture 1
// Tracks browser Pointer Lock for set_cursor_grab. Appended to mq_js_bundle.js.
// States: 0 idle, 1 requested, 2 active, 3 denied, 4 lost.
(function () {
    "use strict";
    const canvas = document.querySelector("#glcanvas");
    if (!canvas || typeof miniquad_add_plugin !== "function") return;

    const Idle = 0, Requested = 1, Active = 2, Denied = 3, Lost = 4;
    const originalRequest = (canvas.requestPointerLock || function () {}).bind(canvas);
    const originalExit = (document.exitPointerLock || function () {}).bind(document);

    let state = Idle;
    let wanted = false;
    let pending = false;
    let gesture = 0;
    let consumed = 0;
    let attempt = 0;
    let exiting = false;

    function locked() {
        return document.pointerLockElement === canvas || document.mozPointerLockElement === canvas;
    }
    function transition(next) {
        state = next;
    }
    function exitLock() {
        if (!locked() || exiting) return;
        exiting = true;
        try {
            originalExit();
            if (!locked()) exiting = false;
        } catch (error) { exiting = false; }
    }
    function finishFailure(next) {
        pending = false;
        consumed = gesture;
        transition(wanted ? next : Idle);
    }
    function tryRequest(fromGesture) {
        if (!wanted) return;
        if (locked() || state === Active) {
            pending = false;
            transition(Active);
            return;
        }
        // One in-flight request. A second call while it is open is not another
        // browser request, including the per-frame path miniquad used to take.
        if (pending || state === Requested) return;
        // Denied and lost stay put until a pointer or touch press asks again.
        if ((state === Denied || state === Lost) && !fromGesture && gesture <= consumed) return;

        consumed = gesture;
        pending = true;
        const token = ++attempt;
        transition(Requested);
        let result;
        try {
            result = originalRequest();
        } catch (error) {
            if (token === attempt) finishFailure(Denied);
            return;
        }
        if (!result || typeof result.then !== "function") return;
        result.then(function () {
            if (token !== attempt) {
                // Cancellation cannot cancel the browser's in-flight request.
                // Release its late acquisition unless a newer request wants it.
                if (locked() && (!wanted || !pageFocused())) onLockChange();
                return;
            }
            pending = false;
            if (locked()) onLockChange();
            else finishFailure(Denied);
        }, function () {
            if (token !== attempt) return;
            if (locked() && wanted && pageFocused()) {
                pending = false;
                transition(Active);
                return;
            }
            finishFailure(Denied);
        });
    }
    function release() {
        wanted = false;
        pending = false;
        attempt++;
        exitLock();
        transition(Idle);
    }
    function isEscape(event) {
        return event && (event.code === "Escape" || event.key === "Escape");
    }
    function onGesture(event) {
        if (event && event.repeat) return;
        if (isEscape(event)) return;
        // An active or in-flight lock does not spend a new gesture. Escape is
        // not a recapture press; the browser uses it to drop the lock.
        if (!wanted || state === Active || state === Requested) return;
        gesture++;
        tryRequest(true);
    }
    // Capture phase runs before the page's click handler. pointerdown and
    // mousedown both fire for one click; a pending request drops the second.
    for (const name of ["pointerdown", "mousedown", "touchstart"]) {
        window.addEventListener(name, onGesture, true);
    }
    function onLockChange() {
        if (locked()) {
            pending = false;
            if (!wanted || !pageFocused()) {
                transition(wanted ? Lost : Idle);
                exitLock();
                return;
            }
            transition(Active);
            return;
        }
        exiting = false;
        if (!wanted) {
            pending = false;
            transition(Idle);
            return;
        }
        if (state === Active || state === Requested) finishFailure(Lost);
    }
    document.addEventListener("pointerlockchange", onLockChange);
    document.addEventListener("mozpointerlockchange", onLockChange);
    function onLockError() {
        if (locked()) return;
        if (wanted) finishFailure(Denied);
        else transition(Idle);
    }
    document.addEventListener("pointerlockerror", onLockError);
    document.addEventListener("mozpointerlockerror", onLockError);

    function pageFocused() {
        const focus = typeof document.hasFocus === "function" ? document.hasFocus() : true;
        return focus && document.visibilityState !== "hidden";
    }
    let hasFocus = pageFocused();
    function onFocus() {
        const now = pageFocused();
        if (hasFocus === now) return;
        hasFocus = now;
        if (now) return;
        if (state !== Active && state !== Requested && !locked()) return;
        pending = false;
        attempt++;
        exitLock();
        consumed = gesture;
        transition(wanted ? Lost : Idle);
    }
    document.addEventListener("visibilitychange", onFocus);
    window.addEventListener("blur", onFocus);
    window.addEventListener("focus", onFocus);

    // Replace the loader's direct request so a queued sapp_set_cursor_grab
    // uses the same dedupe and gesture gate.
    canvas.requestPointerLock = function () {
        wanted = true;
        tryRequest(false);
    };
    document.exitPointerLock = function () {
        release();
    };

    miniquad_add_plugin({
        name: "macroquad_pointer_capture",
        version: 1,
        register_plugin(imports) {
            imports.env.mq_capture_set_wanted = function (flag) {
                if (flag) {
                    wanted = true;
                    tryRequest(false);
                } else {
                    release();
                }
            };
            imports.env.mq_capture_state = function () { return state; };
            imports.env.mq_capture_ready = function () { return 1; };
        },
    });
})();
