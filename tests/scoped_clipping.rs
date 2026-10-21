#[path = "support/scoped_clipping_fixture.rs"]
mod fixture;

#[macroquad::test]
async fn nested_immediate_clips_restore_and_preserve_deferred_ui() {
    assert!(fixture::run().await > 100_000);
}
