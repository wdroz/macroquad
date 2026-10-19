//! Run with `cargo run --example font_atlas_lifetime`, or compile for
//! wasm32-unknown-unknown and serve font_atlas_lifetime.html. The same fixture
//! runs in tests/font_atlas_lifetime.rs. A pass prints FONT_ATLAS_FIXTURE_PASS
//! and stays green. WASM exports expose status (0 pending, 1 pass, 2 panic),
//! actual DPI, reference lit pixels, and a deterministic reference checksum.

#[path = "../tests/support/font_atlas_fixture.rs"]
mod fixture;

use macroquad::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};

static STATUS: AtomicU32 = AtomicU32::new(0);
static DPI_MILLI: AtomicU32 = AtomicU32::new(0);
static LIT_PIXELS: AtomicU32 = AtomicU32::new(0);
static CHECKSUM: AtomicU32 = AtomicU32::new(0);

#[no_mangle]
pub extern "C" fn font_atlas_fixture_status() -> u32 {
    STATUS.load(Ordering::Relaxed)
}

#[no_mangle]
pub extern "C" fn font_atlas_fixture_dpi_milli() -> u32 {
    DPI_MILLI.load(Ordering::Relaxed)
}

#[no_mangle]
pub extern "C" fn font_atlas_fixture_lit_pixels() -> u32 {
    LIT_PIXELS.load(Ordering::Relaxed)
}

#[no_mangle]
pub extern "C" fn font_atlas_fixture_checksum() -> u32 {
    CHECKSUM.load(Ordering::Relaxed)
}

fn config() -> Conf {
    Conf {
        window_title: "Font atlas lifetime GPU fixture".into(),
        window_width: 512,
        window_height: 128,
        high_dpi: true,
        ..Default::default()
    }
}

#[macroquad::main(config)]
async fn main() {
    std::panic::set_hook(Box::new(|info| {
        STATUS.store(2, Ordering::Relaxed);
        macroquad::logging::error!("FONT_ATLAS_FIXTURE_FAIL: {info}");
    }));
    let result = fixture::run().await;
    DPI_MILLI.store((result.dpi * 1000.).round() as u32, Ordering::Relaxed);
    LIT_PIXELS.store(result.lit_pixels, Ordering::Relaxed);
    CHECKSUM.store(result.checksum, Ordering::Relaxed);
    STATUS.store(1, Ordering::Relaxed);
    macroquad::logging::info!("FONT_ATLAS_FIXTURE_PASS {result:?}");
    loop {
        clear_background(GREEN);
        next_frame().await;
    }
}
