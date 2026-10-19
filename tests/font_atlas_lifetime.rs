#[path = "support/font_atlas_fixture.rs"]
mod fixture;

// One windowed regression per binary because miniquad owns a global display.
#[macroquad::test]
async fn queued_glyphs_survive_atlas_growth_drop_and_collection() {
    let result = fixture::run().await;
    println!(
        "FONT_ATLAS_FIXTURE_PASS dpi={} lit_pixels={} checksum={}",
        result.dpi, result.lit_pixels, result.checksum
    );
}
