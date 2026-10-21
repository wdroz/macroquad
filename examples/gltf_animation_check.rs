//! Self-contained native/WebGL animation fixture; green means checks passed.

#[cfg(feature = "gltf")]
#[path = "../tests/support/gltf_animation_fixture.rs"]
mod fixture;

#[cfg(feature = "gltf")]
use macroquad::prelude::*;

#[cfg(feature = "gltf")]
fn window_conf() -> Conf {
    Conf {
        window_title: "glTF animation clip GPU fixture".to_owned(),
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
        macroquad::logging::error!("GLTF_ANIMATION_FIXTURE_FAIL: {info}");
    }));
    fixture::run().await;
    loop {
        clear_background(GREEN);
        next_frame().await;
    }
}

#[cfg(not(feature = "gltf"))]
fn main() {
    println!("Run with --features gltf for a child-transform and weight clip.");
}
