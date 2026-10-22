#[path = "support/wrapped_labels_fixture.rs"]
mod fixture;

#[macroquad::test]
async fn wrapped_labels_measure_resize_scroll_and_clip() {
    let menu = fixture::run().await;
    assert_eq!(menu.clicks, 1);
    println!(
        "WRAPPED_LABELS_PASS height={} scroll_max={}",
        menu.height, menu.scroll_max
    );
}
