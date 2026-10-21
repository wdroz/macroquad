//! Self-contained native/WebGL morph fixture; green means all checks passed.

#[cfg(feature = "gltf")]
#[path = "../tests/support/gltf_morph_fixture.rs"]
mod fixture;

#[cfg(feature = "gltf")]
use macroquad::prelude::*;

#[cfg(feature = "gltf")]
fn window_conf() -> Conf {
    Conf {
        window_title: "glTF morph geometry GPU fixture".to_owned(),
        window_width: 256,
        window_height: 128,
        high_dpi: true,
        ..Default::default()
    }
}

#[cfg(feature = "gltf")]
#[macroquad::main(window_conf)]
async fn main() {
    #[cfg(target_arch = "wasm32")]
    std::panic::set_hook(Box::new(|info| {
        macroquad::logging::error!("GLTF_MORPH_FIXTURE_FAIL: {info}");
    }));
    fixture::run().await;
    loop {
        clear_background(GREEN);
        next_frame().await;
    }
}

#[cfg(not(feature = "gltf"))]
fn main() {
    println!("Run with --features gltf to check independent glTF morph instances.");
}
