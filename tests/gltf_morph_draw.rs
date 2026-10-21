#![cfg(feature = "gltf")]

#[path = "support/gltf_morph_fixture.rs"]
mod fixture;

#[macroquad::test]
async fn independent_morph_instances_preserve_queued_geometry_and_update_bounds() {
    fixture::run().await;
}
