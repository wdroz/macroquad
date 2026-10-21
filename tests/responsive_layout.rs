#[path = "support/responsive_layout_fixture.rs"]
mod fixture;

#[macroquad::test]
async fn resized_settings_menu_keeps_oversized_controls_reachable() {
    let menu = fixture::run().await;
    assert!(menu.clicks > 0);
    println!(
        "RESPONSIVE_LAYOUT_PASS clicks={} scroll_max={}",
        menu.clicks, menu.scroll_max
    );
}
