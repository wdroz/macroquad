#[path = "support/sprite_animation_fixture.rs"]
mod fixture;
#[path = "../examples/support/sprite_regions_sheet.rs"]
#[allow(dead_code)]
mod sheet;

#[macroquad::test]
async fn caller_timing_selects_expected_sprite_pixels() {
    assert_eq!(fixture::run().await, 72_000);
}
