//! Resize to 390 x 400, scroll the settings, and use the pinned action row.
use macroquad::prelude::*;
use macroquad::ui::{hash, root_ui, widgets, Layout, LayoutAlign, LayoutSettings, Skin};

fn config() -> Conf {
    Conf {
        window_title: "Responsive pause/settings menu".into(),
        window_width: 960,
        window_height: 720,
        high_dpi: true,
        ..Default::default()
    }
}

#[macroquad::main(config)]
async fn main() {
    let skin = {
        let ui = root_ui();
        Skin {
            label_style: ui.style_builder().font_size(18).build(),
            button_style: ui.style_builder().font_size(18).build(),
            ..ui.default_skin()
        }
    };
    let mut enabled = [true, true, false, true, false, true, true, false];
    let mut status = "Paused";
    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        clear_background(Color::from_rgba(24, 35, 51, 255));
        draw_text("Resize / scroll / click", 16., 32., 26., WHITE);
        let size = vec2(
            (screen_width() - 24.).clamp(1., 620.),
            (screen_height() - 76.).max(1.),
        );
        let position = vec2((screen_width() - size.x) * 0.5, 48.);
        let mut ui = root_ui();
        ui.push_skin(&skin);
        widgets::Window::new(hash!("responsive-pause"), position, size)
            .titlebar(false).movable(false).ui(&mut ui, |ui| {
                ui.set_layout(LayoutSettings::new(Layout::Vertical)
                    .padding(RectOffset::new(12., 12., 10., 10.)).gap(vec2(0., 10.)));
                widgets::Label::new("Pause / Settings").size(vec2(ui.available_width(), 24.)).ui(ui);
                let content_width = ui.available_width();
                let content_height = (ui.available_size().y - 50.).max(1.);
                widgets::Group::new(hash!("scroll-settings"), vec2(content_width, content_height))
                    .layout(Layout::Vertical)
                    .content_layout(LayoutSettings::new(Layout::Vertical)
                        .padding(RectOffset::new(10., 10., 10., 16.)).gap(vec2(0., 8.)))
                    .ui(ui, |ui| {
                        // Reuse existing text wrapping and label widgets. Each line
                        // participates in the same column and existing group clip.
                        let wrapped = wrap_text("These settings deliberately exceed the compact viewport. Every option remains reachable by scrolling; the action row stays below this group.", None, 18, 1., ui.available_width());
                        for line in wrapped.lines() {
                            widgets::Label::new(line).size(vec2(ui.available_width(), 22.)).ui(ui);
                        }
                        for (index, value) in enabled.iter_mut().enumerate() {
                            let label = format!("Option {}: {}", index + 1, if *value { "On" } else { "Off" });
                            if widgets::Button::new(label).size(vec2(ui.available_width(), 38.)).ui(ui) {
                                *value = !*value;
                                status = "Setting changed";
                            }
                        }
                        if widgets::Button::new("Final setting: reset all").size(vec2(ui.available_width(), 38.)).ui(ui) {
                            enabled.fill(false);
                            status = "All settings reset";
                        }
                    });
                widgets::Group::new(hash!("actions"), vec2(content_width, 40.))
                    .layout(Layout::Vertical)
                    .content_layout(LayoutSettings::new(Layout::Horizontal)
                        .gap(vec2(10., 0.)).align(LayoutAlign::Center))
                    .ui(ui, |ui| {
                        let half = ((ui.available_width() - 10.) * 0.5).max(0.);
                        if widgets::Button::new("Resume").size(vec2(half, 36.)).ui(ui) { status = "Resumed"; }
                        if widgets::Button::new("Apply").size(vec2(ui.available_width(), 36.)).ui(ui) { status = "Settings applied"; }
                    });
            });
        ui.pop_skin();
        drop(ui);
        draw_text(status, 16., screen_height() - 8., 18., WHITE);
        next_frame().await;
    }
}
