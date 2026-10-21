//! The same built-in UI menu runs in native and WebGL fixtures.
use macroquad::{
    prelude::*,
    ui::{root_ui, widgets, InputHandler, Layout, LayoutAlign, LayoutSettings},
};

#[derive(Default)]
pub struct Menu {
    pub clicks: u32,
    pub setting_clicks: u32,
    pub footer_clicks: u32,
    pub last: Rect,
    pub scroll_max: f32,
    pub scroll_y: f32,
    pub width: f32,
    pub wrapped_lines: usize,
}

impl Menu {
    pub fn draw(&mut self) {
        set_default_camera();
        clear_background(Color::from_rgba(18, 28, 45, 255));
        let mut ui = root_ui();
        let mut skin = ui.default_skin();
        skin.margin = 0.;
        skin.window_style = ui.style_builder().build();
        skin.label_style = ui.style_builder().font_size(16).text_color(WHITE).build();
        skin.group_style = ui
            .style_builder()
            .color(Color::from_rgba(30, 44, 62, 255))
            .build();
        ui.push_skin(&skin);
        let outer = vec2(
            (screen_width() - 24.).max(1.),
            (screen_height() - 72.).max(1.),
        );
        let settings = LayoutSettings::new(Layout::Vertical)
            .padding(RectOffset::new(12., 12., 10., 12.))
            .gap(vec2(8., 7.))
            .align(LayoutAlign::Start);
        widgets::Group::new(macroquad::hash!("responsive-settings-fixture"), outer)
            .position(vec2(12., 12.))
            .content_layout(settings)
            .ui(&mut ui, |ui| {
                self.width = ui.available_width();
                assert!(self.width > 0. && self.width <= outer.x - 24. + 0.01);
                let description = wrap_text(
                    "Pause and settings: this intentionally oversized menu keeps every action reachable after resizing. Scroll to the final Apply settings button.",
                    None, 16, 1., self.width - 10.,
                );
                self.wrapped_lines = description.lines().count();
                for line in description.lines() {
                    widgets::Label::new(line).size(vec2(self.width, 22.)).ui(ui);
                }
                for index in 0..15 {
                    let label = format!("Setting {}: normal", index + 1);
                    if widgets::Button::new(label).size(vec2(ui.available_width(), 30.)).ui(ui) {
                        self.setting_clicks += 1;
                    }
                }
                self.width = ui.available_width();
                let position = ui.canvas().cursor();
                self.last = Rect::new(position.x, position.y, self.width, 34.);
                if widgets::Button::new("Apply settings").size(vec2(self.width, 34.)).ui(ui) {
                    self.clicks += 1;
                }
                self.scroll_max = ui.scroll_max().y.max(0.);
                self.scroll_y = ui.scroll().y;
            });
        // Ordinary root calls retain legacy layout and remain outside the scroll group.
        if widgets::Button::new(format!("Unclipped footer | applies {}", self.clicks))
            .position(vec2(12., screen_height() - 44.))
            .size(vec2((screen_width() - 24.).max(1.), 32.))
            .ui(&mut ui)
        {
            self.footer_clicks += 1;
        }
        ui.pop_skin();
    }
}

/// Scripted widget input here uses the public UI input interface; browser automation
/// additionally delivers actual DOM pointer/wheel events to the displayed menu.
pub async fn run() -> Menu {
    let mut menu = Menu::default();
    #[cfg(not(target_arch = "wasm32"))]
    let sizes = [vec2(390., 400.), vec2(960., 640.), vec2(390., 400.)];
    #[cfg(target_arch = "wasm32")]
    let sizes = [vec2(screen_width(), screen_height())];
    for (index, size) in sizes.into_iter().enumerate() {
        #[cfg(not(target_arch = "wasm32"))]
        request_new_screen_size(size.x, size.y);
        for _ in 0..5 {
            menu.draw();
            next_frame().await;
        }
        assert!((screen_width() - size.x).abs() <= 1. && (screen_height() - size.y).abs() <= 1.);
        assert!(
            menu.scroll_max > 0.,
            "oversized menu must use existing scrolling"
        );
        if size.x <= 390. {
            assert!(menu.wrapped_lines >= 3, "small menu labels must wrap");
        }
        let footer = (screen_width() * 0.5, screen_height() - 28.);
        let settings_before = menu.setting_clicks;
        root_ui().mouse_move(footer);
        root_ui().mouse_down(footer);
        menu.draw();
        root_ui().mouse_up(footer);
        next_frame().await;
        assert_eq!(
            menu.setting_clicks, settings_before,
            "clipped settings must not also activate when clicking the pinned footer"
        );
        for _ in 0..4 {
            root_ui().mouse_move((screen_width() * 0.5, screen_height() * 0.5));
            root_ui().mouse_wheel(0., 10_000.);
            menu.draw();
            next_frame().await;
        }
        assert!(
            menu.last.y >= 12. && menu.last.bottom() <= screen_height() - 60. + 0.1,
            "last control must be reachable: {:?}, viewport {:?}, scroll {} / {}",
            menu.last,
            size,
            menu.scroll_y,
            menu.scroll_max
        );
        assert!(
            menu.last.x >= 24. && menu.last.right() <= screen_width() - 24. - 10. + 0.1,
            "full-width control must leave scrollbar room: {:?}",
            menu.last
        );
        let point = (
            menu.last.x + menu.last.w * 0.5,
            menu.last.y + menu.last.h * 0.5,
        );
        let before = menu.clicks;
        root_ui().mouse_move(point);
        root_ui().mouse_down(point);
        menu.draw();
        root_ui().mouse_up(point);
        next_frame().await;
        assert_eq!(
            menu.clicks,
            before + 1,
            "last built-in button must accept pointer activation"
        );
        #[cfg(not(target_arch = "wasm32"))]
        if let Ok(directory) = std::env::var("MACROQUAD_LAYOUT_CAPTURE_DIR") {
            // Deferred root UI is visible only after a frame boundary. Read the
            // already presented frame rather than claiming queued widgets were captured.
            get_screen_data().export_png(&format!(
                "{directory}/layout-{index}-{}x{}.png",
                size.x, size.y
            ));
        }
        #[cfg(target_arch = "wasm32")]
        let _ = index;
    }
    check_resize_scroll().await;
    menu
}

/// A newly enlarged group must use its clamped offset on the first resized frame.
async fn check_resize_scroll() {
    let draw = |height: f32| {
        let mut first_y = 0.;
        let mut scroll_y = 0.;
        widgets::Group::new(
            macroquad::hash!("resize-scroll-regression"),
            vec2(240., height),
        )
        .position(vec2(12., 12.))
        .content_layout(LayoutSettings::new(Layout::Vertical))
        .ui(&mut root_ui(), |ui| {
            first_y = ui.canvas().cursor().y;
            for _ in 0..8 {
                widgets::Button::new("Resize check")
                    .size(vec2(200., 30.))
                    .ui(ui);
            }
            scroll_y = ui.scroll().y;
        });
        (first_y, scroll_y)
    };
    let mut unscrolled_y = 0.;
    for _ in 0..3 {
        unscrolled_y = draw(100.).0;
        next_frame().await;
    }
    root_ui().mouse_move((50., 50.));
    root_ui().mouse_wheel(0., 10_000.);
    draw(100.);
    next_frame().await;
    assert!(draw(100.).1 < 0., "fixture must start scrolled down");
    next_frame().await;
    let (first_y, scroll_y) = draw(300.);
    assert!(
        (first_y - unscrolled_y).abs() < 0.01,
        "resizing must immediately restore the first control: {first_y}"
    );
    assert_eq!(
        scroll_y, 0.,
        "a taller viewport must not retain stale or negative bounds"
    );
    next_frame().await;
}
