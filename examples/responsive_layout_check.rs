//! Native/WebGL regression for resized built-in UI groups and scrolling.
#[path = "../tests/support/responsive_layout_fixture.rs"]
mod fixture;
use macroquad::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
static STATUS: AtomicU32 = AtomicU32::new(0);
static CLICKS: AtomicU32 = AtomicU32::new(0);
static X: AtomicU32 = AtomicU32::new(0);
static Y: AtomicU32 = AtomicU32::new(0);
static SCROLL: AtomicU32 = AtomicU32::new(0);
static SETTINGS: AtomicU32 = AtomicU32::new(0);
static FOOTER: AtomicU32 = AtomicU32::new(0);
#[no_mangle]
pub extern "C" fn responsive_layout_status() -> u32 {
    STATUS.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn responsive_layout_clicks() -> u32 {
    CLICKS.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn responsive_layout_last_x() -> u32 {
    X.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn responsive_layout_last_y() -> u32 {
    Y.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn responsive_layout_scroll() -> u32 {
    SCROLL.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn responsive_layout_settings_clicks() -> u32 {
    SETTINGS.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn responsive_layout_footer_clicks() -> u32 {
    FOOTER.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn responsive_layout_dpi_milli() -> u32 {
    (screen_dpi_scale() * 1000.).round() as u32
}
fn config() -> Conf {
    Conf {
        window_title: "Responsive layout checks".into(),
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
        macroquad::logging::error!("RESPONSIVE_LAYOUT_FAIL: {}", info);
    }));
    let mut menu = fixture::run().await;
    STATUS.store(1, Ordering::Relaxed);
    macroquad::logging::info!("RESPONSIVE_LAYOUT_PASS");
    loop {
        menu.draw();
        CLICKS.store(menu.clicks, Ordering::Relaxed);
        X.store(
            (menu.last.x + menu.last.w * 0.5).max(0.).round() as u32,
            Ordering::Relaxed,
        );
        Y.store(
            (menu.last.y + menu.last.h * 0.5).max(0.).round() as u32,
            Ordering::Relaxed,
        );
        SCROLL.store((-menu.scroll_y).max(0.).round() as u32, Ordering::Relaxed);
        SETTINGS.store(menu.setting_clicks, Ordering::Relaxed);
        FOOTER.store(menu.footer_clicks, Ordering::Relaxed);
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        next_frame().await;
    }
}
