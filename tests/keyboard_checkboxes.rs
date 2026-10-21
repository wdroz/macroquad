#[path = "support/keyboard_checkboxes_fixture.rs"]
mod fixture;

#[macroquad::test]
async fn keyboard_checkboxes_share_focus_with_editboxes_and_modal() {
    let menu = fixture::run().await;
    assert_eq!(menu.disabled_clicks, 0);
    println!(
        "KEYBOARD_CHECKBOXES_PASS apply={} confirm={} legacy={}",
        menu.apply, menu.keep, menu.legacy
    );
}
