#[path = "support/rounded_panels_fixture.rs"]
mod fixture;

#[macroquad::test]
async fn translucent_rounded_panels_have_single_coverage() {
    assert!(fixture::run() > 100_000);
}
