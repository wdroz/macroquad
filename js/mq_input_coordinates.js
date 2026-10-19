// macroquad input coordinates 1
// Keep input in framebuffer pixels; rendering and frame scheduling are unchanged.
function macroquad_input_scale() {
    const rect = canvas.getBoundingClientRect();
    const density = dpi_scale();
    return {
        rect,
        x: rect.width > 0 ? macroquad_canvas_width(canvas) / rect.width : density,
        y: rect.height > 0 ? macroquad_canvas_height(canvas) / rect.height : density,
        density,
    };
}

function macroquad_input_position(clientX, clientY) {
    const scale = macroquad_input_scale();
    return {
        x: (clientX - scale.rect.left) * scale.x,
        y: (clientY - scale.rect.top) * scale.y,
    };
}

// miniquad's exported raw_mouse_move takes i32. Accumulate subpixel motion
// instead of rounding every browser event (especially negative motion).
const macroquad_raw_motion = { x: 0, y: 0, signature: null };
function macroquad_input_raw_motion(movementX, movementY) {
    const scale = macroquad_input_scale();
    const signature = [macroquad_canvas_width(canvas), macroquad_canvas_height(canvas), scale.rect.width,
        scale.rect.height, scale.density, document.pointerLockElement,
        document.mozPointerLockElement];
    if (!macroquad_raw_motion.signature || signature.some(
        (value, index) => value !== macroquad_raw_motion.signature[index]
    )) {
        macroquad_raw_motion.x = macroquad_raw_motion.y = 0;
        macroquad_raw_motion.signature = signature;
    }
    if (!Number.isFinite(movementX) || !Number.isFinite(movementY)) return;
    macroquad_raw_motion.x += movementX * scale.x;
    macroquad_raw_motion.y += movementY * scale.y;
    const x = Math.trunc(macroquad_raw_motion.x);
    const y = Math.trunc(macroquad_raw_motion.y);
    macroquad_raw_motion.x -= x;
    macroquad_raw_motion.y -= y;
    if (x !== 0 || y !== 0) wasm_exports.raw_mouse_move(x, y);
}

for (const type of ["pointerlockchange", "mozpointerlockchange"]) {
    document.addEventListener(type, function () {
        macroquad_raw_motion.signature = null;
    });
}
