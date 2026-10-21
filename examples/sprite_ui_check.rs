//! Shared flow/pixel fixture, then the live example for real native/DOM input.
#[path = "../tests/support/sprite_ui_fixture.rs"]
mod fixture;
#[path = "support/sprite_ui_scene.rs"]
mod scene;
#[path = "support/sprite_regions_sheet.rs"]
#[allow(dead_code)]
mod sheet;
use macroquad::{prelude::*, ui::root_ui};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

static STATUS: AtomicU32 = AtomicU32::new(0);
static CHECKS: AtomicU32 = AtomicU32::new(0);
static FRAMES: AtomicU32 = AtomicU32::new(0);
static PAUSED: AtomicU32 = AtomicU32::new(0);
static FRAME: AtomicU32 = AtomicU32::new(0);
static HITS: AtomicU32 = AtomicU32::new(0);
static RESTARTS: AtomicU32 = AtomicU32::new(0);
static FOCUS: AtomicU32 = AtomicU32::new(0);
static ELAPSED: AtomicU64 = AtomicU64::new(0);
static SCROLL: AtomicU32 = AtomicU32::new(0);
static POSITION: [AtomicU32; 2] = [const { AtomicU32::new(0) }; 2];
static RECTANGLES: [AtomicU32; 24] = [const { AtomicU32::new(0) }; 24];
macro_rules! probe {
    ($name:ident, $value:ident) => {
        #[no_mangle]
        pub extern "C" fn $name() -> u32 {
            $value.load(Ordering::Relaxed)
        }
    };
}
probe!(sprite_ui_status, STATUS);
probe!(sprite_ui_checks, CHECKS);
probe!(sprite_ui_frames, FRAMES);
probe!(sprite_ui_paused, PAUSED);
probe!(sprite_ui_frame, FRAME);
probe!(sprite_ui_hits, HITS);
probe!(sprite_ui_restarts, RESTARTS);
probe!(sprite_ui_focus, FOCUS);
#[no_mangle]
pub extern "C" fn sprite_ui_elapsed() -> f64 {
    f64::from_bits(ELAPSED.load(Ordering::Relaxed))
}
#[no_mangle]
pub extern "C" fn sprite_ui_scroll() -> f32 {
    f32::from_bits(SCROLL.load(Ordering::Relaxed))
}
#[no_mangle]
pub extern "C" fn sprite_ui_sprite(component: u32) -> f32 {
    POSITION
        .get(component as usize)
        .map_or(0., |n| f32::from_bits(n.load(Ordering::Relaxed)))
}
#[no_mangle]
pub extern "C" fn sprite_ui_rect(kind: u32, component: u32) -> f32 {
    if kind >= 6 || component >= 4 {
        return 0.;
    }
    f32::from_bits(RECTANGLES[(kind * 4 + component) as usize].load(Ordering::Relaxed))
}
#[no_mangle]
pub extern "C" fn sprite_ui_dpi_milli() -> u32 {
    (screen_dpi_scale() * 1000.).round() as u32
}

fn config() -> Conf {
    Conf {
        window_width: 390,
        window_height: 400,
        ..scene::config()
    }
}

#[macroquad::main(config)]
async fn main() {
    std::panic::set_hook(Box::new(|info| {
        STATUS.store(2, Ordering::Relaxed);
        macroquad::logging::error!("SPRITE_UI_FAIL: {}", info);
    }));
    CHECKS.store(fixture::run().await, Ordering::Relaxed);
    STATUS.store(1, Ordering::Relaxed);
    macroquad::logging::info!("SPRITE_UI_PASS");
    let mut scene = scene::Scene::new();
    loop {
        scene.frame(f64::from(get_frame_time()));
        FRAMES.fetch_add(1, Ordering::Relaxed);
        PAUSED.store(u32::from(scene.paused), Ordering::Relaxed);
        FRAME.store(scene.sprite.current_frame(), Ordering::Relaxed);
        HITS.store(scene.hits, Ordering::Relaxed);
        RESTARTS.store(scene.restarts, Ordering::Relaxed);
        FOCUS.store(
            root_ui().focused_widget().unwrap_or(0) as u32,
            Ordering::Relaxed,
        );
        ELAPSED.store(scene.sprite.elapsed().to_bits(), Ordering::Relaxed);
        SCROLL.store(scene.scroll.to_bits(), Ordering::Relaxed);
        let point = scene.sprite_window_position();
        POSITION[0].store(point.x.to_bits(), Ordering::Relaxed);
        POSITION[1].store(point.y.to_bits(), Ordering::Relaxed);
        for (kind, rect) in scene.rectangles.iter().enumerate() {
            for (component, value) in [rect.x, rect.y, rect.w, rect.h].into_iter().enumerate() {
                RECTANGLES[kind * 4 + component].store(value.to_bits(), Ordering::Relaxed);
            }
        }
        // F12 gives the native input harness the same read-only state as WASM.
        #[cfg(not(target_arch = "wasm32"))]
        if is_key_pressed(KeyCode::F12) {
            let rectangles: Vec<_> = scene
                .rectangles
                .iter()
                .map(|r| [r.x, r.y, r.w, r.h])
                .collect();
            println!("SPRITE_UI_SNAPSHOT {{\"frames\":{},\"paused\":{},\"frame\":{},\"hits\":{},\"restarts\":{},\"focus\":{},\"elapsed\":{},\"scroll\":{},\"width\":{},\"height\":{},\"sprite\":{:?},\"rects\":{:?}}}",
                sprite_ui_frames(), scene.paused, scene.sprite.current_frame(), scene.hits,
                scene.restarts, sprite_ui_focus(), scene.sprite.elapsed(), scene.scroll,
                screen_width(), screen_height(), [point.x, point.y], rectangles);
        }
        next_frame().await;
    }
}
