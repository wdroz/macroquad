#[path = "support/sprite_regions_fixture.rs"]
mod fixture;
#[path = "../examples/support/sprite_regions_sheet.rs"]
mod sheet;
#[macroquad::test]
async fn sprite_regions_match_grid_trim_flips_rotation_and_atlas_pixels() {
    assert_eq!(fixture::run().await, 6_389_760);
}
