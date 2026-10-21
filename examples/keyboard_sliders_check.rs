//! Compact keyboard settings demo plus shared native/WebGL public-input checks.
#[path = "../tests/support/keyboard_sliders_fixture.rs"]
mod fixture;
use macroquad::{prelude::*, ui::root_ui};
use std::sync::atomic::{AtomicU32, Ordering};
static STATUS: AtomicU32 = AtomicU32::new(0);
static FOCUS: AtomicU32 = AtomicU32::new(0);
static VALUE: AtomicU32 = AtomicU32::new(0);
static MODAL_VALUE: AtomicU32 = AtomicU32::new(0);
static COMMAND: AtomicU32 = AtomicU32::new(0);
static FRAMES: AtomicU32 = AtomicU32::new(0);
static RECT: [AtomicU32; 4] = [const { AtomicU32::new(0) }; 4];
#[no_mangle]
pub extern "C" fn keyboard_sliders_rect(component: u32) -> u32 {
    RECT.get(component as usize)
        .map_or(0, |v| v.load(Ordering::Relaxed))
}
#[no_mangle]
pub extern "C" fn keyboard_sliders_status() -> u32 {
    STATUS.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn keyboard_sliders_focus() -> u32 {
    FOCUS.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn keyboard_sliders_value() -> u32 {
    VALUE.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn keyboard_sliders_modal_value() -> u32 {
    MODAL_VALUE.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn keyboard_sliders_frames() -> u32 {
    FRAMES.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn keyboard_sliders_command(n: u32) {
    COMMAND.store(n, Ordering::Relaxed);
}
fn config() -> Conf {
    Conf {
        window_title: "Keyboard slider checks".into(),
        window_width: 390,
        window_height: 400,
        high_dpi: true,
        ..Default::default()
    }
}
#[macroquad::main(config)]
async fn main() {
    std::panic::set_hook(Box::new(|info| {
        STATUS.store(2, Ordering::Relaxed);
        macroquad::logging::error!("KEYBOARD_SLIDERS_FAIL: {}", info);
    }));
    let mut menu = fixture::run().await;
    STATUS.store(1, Ordering::Relaxed);
    #[cfg(not(target_arch = "wasm32"))]
    let mut last_state = None;
    loop {
        match COMMAND.swap(0, Ordering::Relaxed) {
            1 => root_ui().clear_input_focus(),
            2 => menu.enabled = false,
            3 => menu.enabled = true,
            _ => {}
        }
        menu.draw();
        for (n, value) in [menu.rect.x, menu.rect.y, menu.rect.w, menu.rect.h]
            .into_iter()
            .enumerate()
        {
            RECT[n].store(value.round() as u32, Ordering::Relaxed);
        }
        FOCUS.store(
            root_ui().focused_widget().unwrap_or(0) as u32,
            Ordering::Relaxed,
        );
        #[cfg(not(target_arch = "wasm32"))]
        {
            let state = (root_ui().focused_widget(), menu.value, menu.modal_value);
            if last_state != Some(state) {
                println!(
                    "SLIDER_STATE focus={:?} value={:.3} modal={:.3}",
                    state.0, state.1, state.2
                );
                last_state = Some(state);
            }
        }
        VALUE.store((menu.value * 1000.).round() as u32, Ordering::Relaxed);
        MODAL_VALUE.store((menu.modal_value * 1000.).round() as u32, Ordering::Relaxed);
        FRAMES.fetch_add(1, Ordering::Relaxed);
        next_frame().await;
    }
}
