#[path = "support/keyboard_sliders_fixture.rs"]
mod fixture;

#[macroquad::test]
async fn keyboard_sliders_share_traversal_and_preserve_value_contract() {
    fixture::run().await;
}
