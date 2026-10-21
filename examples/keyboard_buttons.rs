//! Tab and Shift+Tab traverse buttons and the editbox; Enter/Space activate.
use macroquad::prelude::*;
use macroquad::ui::{hash, root_ui, widgets, Layout, LayoutSettings};

fn config() -> Conf {
    Conf {
        window_title: "Keyboard-operable buttons".into(),
        window_width: 700,
        window_height: 480,
        high_dpi: true,
        ..Default::default()
    }
}

#[macroquad::main(config)]
async fn main() {
    let mut name = "Player".to_string();
    let mut saved = 0;
    let mut confirmation = false;
    let mut status = "Tab or click to focus a control";
    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        clear_background(Color::from_rgba(22, 32, 48, 255));
        draw_text("Keyboard-ready settings", 20., 34., 28., WHITE);
        let width = (screen_width() - 32.).clamp(1., 480.);
        let mut ui = root_ui();
        widgets::Window::new(hash!("settings"), vec2(16., 54.), vec2(width, 262.))
            .titlebar(false)
            .movable(false)
            .ui(&mut ui, |ui| {
                ui.set_layout(
                    LayoutSettings::new(Layout::Vertical)
                        .padding(RectOffset::new(12., 12., 12., 12.))
                        .gap(vec2(0., 10.)),
                );
                widgets::Label::new("Name")
                    .size(vec2(ui.available_width(), 20.))
                    .ui(ui);
                widgets::Editbox::new(hash!("name"), vec2(ui.available_width(), 32.))
                    .multiline(false)
                    .ui(ui, &mut name);
                if widgets::Button::new(format!("Save name ({saved})"))
                    .id(hash!("save"))
                    .size(vec2(ui.available_width(), 36.))
                    .enabled(!name.trim().is_empty())
                    .ui(ui)
                {
                    saved += 1;
                    status = "Saved: held Enter/Space does not repeat";
                }
                if widgets::Button::new("Reset settings...")
                    .id(hash!("reset"))
                    .size(vec2(ui.available_width(), 36.))
                    .ui(ui)
                {
                    confirmation = true;
                }
                widgets::Button::new("Unavailable action")
                    .id(hash!("disabled"))
                    .enabled(false)
                    .size(vec2(ui.available_width(), 36.))
                    .ui(ui);
            });
        if confirmation {
            let width = (screen_width() - 72.).clamp(1., 340.);
            ui.modal(hash!("confirm"), vec2(36., 104.), vec2(width, 154.), |ui| {
                ui.canvas()
                    .rect(Rect::new(36., 104., width, 154.), DARKGRAY, LIGHTGRAY);
                ui.set_layout(
                    LayoutSettings::new(Layout::Vertical)
                        .padding(RectOffset::new(12., 12., 12., 12.))
                        .gap(vec2(0., 10.)),
                );
                widgets::Label::new("Reset this name?")
                    .size(vec2(ui.available_width(), 22.))
                    .ui(ui);
                if widgets::Button::new("Reset")
                    .id(hash!("confirm-reset"))
                    .size(vec2(ui.available_width(), 32.))
                    .ui(ui)
                {
                    name = "Player".into();
                    saved = 0;
                    confirmation = false;
                    status = "Reset; focus returns to the opener";
                }
                if widgets::Button::new("Cancel")
                    .id(hash!("cancel"))
                    .size(vec2(ui.available_width(), 32.))
                    .ui(ui)
                {
                    confirmation = false;
                    status = "Canceled; focus returns to the opener";
                }
            });
        }
        drop(ui);
        draw_text(
            "Tab / Shift+Tab to move; Enter / Space to activate",
            16.,
            346.,
            16.,
            WHITE,
        );
        draw_text(status, 16., 372., 16., YELLOW);
        next_frame().await;
    }
}
