#[path = "support/rounded_outlines_fixture.rs"]
mod fixture;

#[macroquad::test]
async fn inward_outlines_have_single_coverage_and_preserve_state() {
    assert_eq!(fixture::run(), 4_718_592);
}
