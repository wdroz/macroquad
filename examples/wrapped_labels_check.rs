//! Resize and scroll this asset-free menu; shared checks also run on WebGL.
#[path = "../tests/support/wrapped_labels_fixture.rs"]
mod fixture;
use macroquad::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
static STATUS: AtomicU32 = AtomicU32::new(0);
static FRAMES: AtomicU32 = AtomicU32::new(0);
static CLICKS: AtomicU32 = AtomicU32::new(0);
static FOOTER: AtomicU32 = AtomicU32::new(0);
static GEOMETRY: [AtomicU32; 8] = [const { AtomicU32::new(0) }; 8];
#[no_mangle]
pub extern "C" fn wrapped_labels_status() -> u32 {
    STATUS.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn wrapped_labels_frames() -> u32 {
    FRAMES.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn wrapped_labels_clicks() -> u32 {
    CLICKS.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn wrapped_labels_footer_clicks() -> u32 {
    FOOTER.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn wrapped_labels_rect(component: u32) -> f32 {
    GEOMETRY
        .get(component as usize)
        .map_or(0., |v| f32::from_bits(v.load(Ordering::Relaxed)))
}
#[no_mangle]
pub extern "C" fn wrapped_labels_width() -> f32 {
    wrapped_labels_rect(4)
}
#[no_mangle]
pub extern "C" fn wrapped_labels_height() -> f32 {
    wrapped_labels_rect(5)
}
#[no_mangle]
pub extern "C" fn wrapped_labels_scroll_max() -> f32 {
    wrapped_labels_rect(6)
}
#[no_mangle]
pub extern "C" fn wrapped_labels_scroll_y() -> f32 {
    wrapped_labels_rect(7)
}
fn config() -> Conf {
    Conf {
        window_title: "Wrapped label checks".into(),
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
        macroquad::logging::error!("WRAPPED_LABELS_FAIL: {}", info);
    }));
    let mut menu = fixture::run().await;
    STATUS.store(1, Ordering::Relaxed);
    #[cfg(not(target_arch = "wasm32"))]
    let mut previous = None;
    loop {
        menu.draw();
        for (index, value) in [
            menu.last.x,
            menu.last.y,
            menu.last.w,
            menu.last.h,
            menu.width,
            menu.height,
            menu.scroll_max,
            menu.scroll_y,
        ]
        .into_iter()
        .enumerate()
        {
            GEOMETRY[index].store(value.to_bits(), Ordering::Relaxed);
        }
        CLICKS.store(menu.clicks, Ordering::Relaxed);
        FOOTER.store(menu.footer_clicks, Ordering::Relaxed);
        FRAMES.fetch_add(1, Ordering::Relaxed);
        #[cfg(not(target_arch = "wasm32"))]
        {
            let state = (
                screen_width() as u32,
                screen_height() as u32,
                menu.clicks,
                menu.footer_clicks,
            );
            if previous != Some(state) {
                println!("WRAPPED_LABELS_STATE size={}x{} height={} width={} scroll={}/{} clicks={} footer={}",
                    state.0, state.1, menu.height, menu.width, menu.scroll_y, menu.scroll_max, state.2, state.3);
                previous = Some(state);
            }
        }
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        next_frame().await;
    }
}
