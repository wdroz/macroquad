//! The tiny imported-part GPU fixture from `tests/gltf_pose.rs`, also runnable
//! in a browser. A passing run prints GLTF_POSE_FIXTURE_PASS and stays green.

#[path = "../tests/support/gltf_pose_fixture.rs"]
#[cfg(feature = "gltf")]
mod fixture;

#[cfg(feature = "gltf")]
use macroquad::prelude::*;

#[cfg(feature = "gltf")]
fn window_conf() -> Conf {
    Conf {
        window_title: "glTF replacement-pose GPU fixture".to_owned(),
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
        macroquad::logging::error!("GLTF_POSE_FIXTURE_FAIL: {info}");
    }));
    fixture::run().await;
    loop {
        clear_background(GREEN);
        next_frame().await;
    }
}

#[cfg(not(feature = "gltf"))]
fn main() {
    println!("Run with --features gltf to check glTF replacement poses.");
}
