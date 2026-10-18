#![cfg(feature = "gltf")]

#[path = "support/gltf_material_fixture.rs"]
mod fixture;

// One windowed test per binary: miniquad keeps a process-wide native display.
#[macroquad::test]
async fn gltf_material_passes_match_independent_pixels_and_retained_geometry() {
    fixture::run().await;
}
