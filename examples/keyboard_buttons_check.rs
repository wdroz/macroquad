//! Public input-interface checks, followed by a live menu for real OS/DOM input.
#[path = "../tests/support/keyboard_buttons_fixture.rs"]
mod fixture;
use macroquad::{prelude::*, ui::root_ui};
use std::sync::atomic::{AtomicU32, Ordering};

static STATUS: AtomicU32 = AtomicU32::new(0);
static FOCUS: AtomicU32 = AtomicU32::new(0);
static APPLY: AtomicU32 = AtomicU32::new(0);
static EXTRA: AtomicU32 = AtomicU32::new(0);
static KEEP: AtomicU32 = AtomicU32::new(0);
static LEGACY: AtomicU32 = AtomicU32::new(0);
static MODAL: AtomicU32 = AtomicU32::new(0);
static TEXT_LEN: AtomicU32 = AtomicU32::new(0);
static COMMAND: AtomicU32 = AtomicU32::new(0);
static FRAMES: AtomicU32 = AtomicU32::new(0);
static RECTANGLES: [AtomicU32; 32] = [const { AtomicU32::new(0) }; 32];

#[no_mangle]
pub extern "C" fn keyboard_buttons_status() -> u32 {
    STATUS.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn keyboard_buttons_focus() -> u32 {
    FOCUS.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn keyboard_buttons_apply() -> u32 {
    APPLY.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn keyboard_buttons_extra() -> u32 {
    EXTRA.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn keyboard_buttons_keep() -> u32 {
    KEEP.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn keyboard_buttons_legacy() -> u32 {
    LEGACY.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn keyboard_buttons_modal() -> u32 {
    MODAL.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn keyboard_buttons_text_len() -> u32 {
    TEXT_LEN.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn keyboard_buttons_rect(slot: u32, component: u32) -> u32 {
    RECTANGLES
        .get((slot * 4 + component) as usize)
        .map_or(0, |n| n.load(Ordering::Relaxed))
}
#[no_mangle]
pub extern "C" fn keyboard_buttons_dpi_milli() -> u32 {
    (screen_dpi_scale() * 1000.).round() as u32
}
/// Controlled model changes are separate from actual DOM keyboard/pointer input.
#[no_mangle]
pub extern "C" fn keyboard_buttons_command(command: u32) {
    COMMAND.store(command, Ordering::Relaxed);
}
#[no_mangle]
pub extern "C" fn keyboard_buttons_pending_command() -> u32 {
    COMMAND.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn keyboard_buttons_frames() -> u32 {
    FRAMES.load(Ordering::Relaxed)
}

fn config() -> Conf {
    Conf {
        window_title: "Keyboard button checks".into(),
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
        macroquad::logging::error!("KEYBOARD_BUTTONS_FAIL: {}", info);
    }));
    let mut menu = fixture::run().await;
    STATUS.store(1, Ordering::Relaxed);
    macroquad::logging::info!("KEYBOARD_BUTTONS_PASS");
    loop {
        match COMMAND.swap(0, Ordering::Relaxed) {
            1 => root_ui().clear_input_focus(),
            2 => menu.show_extra = false,
            3 => menu.show_extra = true,
            4 => menu.apply_enabled = false,
            5 => menu.apply_enabled = true,
            _ => {}
        }
        menu.draw();
        FRAMES.fetch_add(1, Ordering::Relaxed);
        FOCUS.store(
            root_ui().focused_widget().unwrap_or(0) as u32,
            Ordering::Relaxed,
        );
        APPLY.store(menu.apply, Ordering::Relaxed);
        EXTRA.store(menu.extra, Ordering::Relaxed);
        KEEP.store(menu.keep, Ordering::Relaxed);
        LEGACY.store(menu.legacy, Ordering::Relaxed);
        MODAL.store(u32::from(menu.modal), Ordering::Relaxed);
        TEXT_LEN.store(menu.text.len() as u32, Ordering::Relaxed);
        for (slot, rect) in menu.rectangles.iter().enumerate() {
            for (component, value) in [rect.x, rect.y, rect.w, rect.h].into_iter().enumerate() {
                RECTANGLES[slot * 4 + component]
                    .store(value.max(0.).round() as u32, Ordering::Relaxed);
            }
        }
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        next_frame().await;
    }
}
