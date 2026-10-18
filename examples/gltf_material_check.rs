//! The same tiny GPU pixel fixture as `tests/gltf_material.rs`, runnable in a
//! browser. A passing run prints GLTF_MATERIAL_FIXTURE_PASS and stays green.

#[path = "../tests/support/gltf_material_fixture.rs"]
#[cfg(feature = "gltf")]
mod fixture;

#[cfg(feature = "gltf")]
use macroquad::prelude::*;

#[cfg(feature = "gltf")]
#[macroquad::main("glTF material GPU fixture")]
async fn main() {
    #[cfg(target_arch = "wasm32")]
    std::panic::set_hook(Box::new(|info| {
        macroquad::logging::error!("GLTF_MATERIAL_FIXTURE_FAIL: {info}");
    }));
    fixture::run().await;
    macroquad::logging::info!("GLTF_MATERIAL_FIXTURE_PASS");
    loop {
        clear_background(GREEN);
        next_frame().await;
    }
}

#[cfg(not(feature = "gltf"))]
fn main() {
    println!("Run with --features gltf to check glTF material passes.");
}
