#![cfg(feature = "gltf")]

#[path = "support/gltf_animation_fixture.rs"]
mod fixture;

#[macroquad::test]
async fn clip_child_transform_and_morph_weight_draw_together() {
    fixture::run().await;
}
