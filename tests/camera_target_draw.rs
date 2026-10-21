#[path = "support/camera_target_fixture.rs"]
mod fixture;

#[macroquad::test]
async fn target_and_display_picking_match_drawn_pixels() {
    assert_eq!(fixture::run().await, 429);
}
