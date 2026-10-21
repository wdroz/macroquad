#[path = "support/keyboard_buttons_fixture.rs"]
mod fixture;

#[macroquad::test]
async fn keyboard_buttons_share_focus_with_editboxes_and_modal() {
    let menu = fixture::run().await;
    assert_eq!(menu.disabled_clicks, 0);
    println!(
        "KEYBOARD_BUTTONS_PASS apply={} confirm={} legacy={}",
        menu.apply, menu.keep, menu.legacy
    );
}
