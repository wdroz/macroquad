//! Shared native/WebGL public-input regression and interactive compact menu.
use macroquad::{
    prelude::*,
    ui::{hash, root_ui, widgets, InputHandler, KeyCode as UiKey},
};

pub const SLIDER: u64 = 201;
pub const CHECK: u64 = 202;
pub const OPEN: u64 = 203;
pub const MODAL_SLIDER: u64 = 204;
pub const CLOSE: u64 = 205;
pub const LEGACY: u64 = 206;

pub struct Menu {
    pub value: f32,
    pub modal_value: f32,
    pub legacy: f32,
    pub checked: bool,
    pub modal: bool,
    pub enabled: bool,
    pub show: bool,
    pub step: f32,
    pub range: std::ops::Range<f32>,
    pub rect: Rect,
    pub legacy_rect: Rect,
    pub clipped: bool,
    pub track_width: Option<f32>,
}

impl Menu {
    pub fn new() -> Self {
        Self {
            value: 0.5,
            modal_value: 0.5,
            legacy: 0.5,
            checked: false,
            modal: false,
            enabled: true,
            show: true,
            step: 0.1,
            range: 0.0..1.0,
            rect: Rect::default(),
            legacy_rect: Rect::default(),
            clipped: false,
            track_width: None,
        }
    }

    pub fn draw(&mut self) {
        clear_background(Color::from_rgba(18, 27, 42, 255));
        draw_text("Tab / arrows / Home / End", 12., 28., 22., WHITE);
        let width = (screen_width() - 24.).clamp(1., 520.);
        let mut ui = root_ui();
        widgets::Window::new(920, vec2(12., 44.), vec2(width, 280.))
            .movable(false)
            .titlebar(false)
            .ui(&mut ui, |ui| {
                ui.set_layout(
                    macroquad::ui::LayoutSettings::new(macroquad::ui::Layout::Vertical)
                        .padding(RectOffset::new(8., 8., 8., 8.))
                        .gap(vec2(0., 10.)),
                );
                widgets::Label::new("Text field, then keyboard track").ui(ui);
                let p = ui.canvas().cursor();
                // Slider's default margin is 3x and label/editbox reserve 150px.
                let size = ui.available_width();
                self.rect = Rect::new(p.x + 51., p.y, size - 150., 20.);
                if self.clipped {
                    ui.group(929, vec2(300., 1.), |ui| {
                        widgets::Label::new("Offscreen spacer").ui(ui);
                        self.slider(ui);
                    });
                } else if self.show {
                    self.slider(ui);
                }
                widgets::Checkbox::new(CHECK)
                    .keyboard_focus(true)
                    .label("Enabled sound")
                    .ui(ui, &mut self.checked);
                if widgets::Button::new("Open settings")
                    .id(OPEN)
                    .size(vec2(180., 30.))
                    .ui(ui)
                {
                    self.modal = true;
                }
                widgets::Label::new("Legacy pointer/text slider").ui(ui);
                let p = ui.canvas().cursor();
                self.legacy_rect = Rect::new(p.x + 51., p.y, ui.available_width() - 150., 20.);
                widgets::Slider::new(LEGACY, 0.0..1.0)
                    .label("Legacy")
                    .ui(ui, &mut self.legacy);
            });
        if self.modal {
            ui.modal(921, vec2(24., 110.), vec2(width - 24., 140.), |ui| {
                ui.canvas().rect(
                    Rect::new(24., 110., width - 24., 140.),
                    WHITE,
                    Color::from_rgba(35, 50, 75, 255),
                );
                widgets::Slider::new(MODAL_SLIDER, 0.0..1.0)
                    .keyboard_focus(true)
                    .keyboard_step(0.25)
                    .label("Modal")
                    .focus_color(YELLOW)
                    .ui(ui, &mut self.modal_value);
                if widgets::Button::new("Close").id(CLOSE).ui(ui) {
                    self.modal = false;
                }
            });
        }
    }

    fn slider(&mut self, ui: &mut macroquad::ui::Ui) {
        let label_width = self
            .track_width
            .map_or(100., |w| ui.available_width() - 50. - w);
        widgets::Slider::new(SLIDER, self.range.clone())
            .keyboard_focus(true)
            .keyboard_step(self.step)
            .label_width(label_width)
            .enabled(self.enabled)
            .focus_color(YELLOW)
            .label("Volume")
            .ui(ui, &mut self.value);
    }
}

pub async fn settle(menu: &mut Menu) {
    for _ in 0..3 {
        menu.draw();
        next_frame().await;
    }
}
pub async fn press(menu: &mut Menu, key: UiKey, shift: bool) {
    root_ui().key_down(key, shift, false);
    menu.draw();
    next_frame().await;
    root_ui().key_up(key);
    settle(menu).await;
}

async fn drag(menu: &mut Menu, rect: Rect) {
    let start = (rect.x + 4. + rect.w * 0.5, rect.y + 9.);
    root_ui().mouse_move(start);
    root_ui().mouse_down(start);
    menu.draw();
    next_frame().await;
    let end = (rect.x + 4. + rect.w * 0.75, rect.y + 9.);
    root_ui().mouse_move(end);
    menu.draw();
    next_frame().await;
    root_ui().mouse_up(end);
    settle(menu).await;
    root_ui().mouse_move((-100., -100.));
}

pub async fn run() -> Menu {
    let mut m = Menu::new();
    settle(&mut m).await;
    root_ui().clear_input_focus();
    press(&mut m, UiKey::Tab, false).await;
    assert_eq!(root_ui().focused_widget(), Some(hash!(SLIDER, "editbox")));
    press(&mut m, UiKey::Tab, false).await;
    assert_eq!(root_ui().focused_widget(), Some(SLIDER));
    press(&mut m, UiKey::Right, false).await;
    assert!((m.value - 0.6).abs() < 1e-6);
    root_ui().key_down(UiKey::Up, false, false);
    settle(&mut m).await;
    let held = m.value;
    for _ in 0..5 {
        root_ui().key_down(UiKey::Up, false, false);
        settle(&mut m).await;
    }
    assert_eq!(m.value, held);
    root_ui().key_up(UiKey::Up);
    press(&mut m, UiKey::Up, false).await;
    assert!((m.value - 0.8).abs() < 1e-6);
    press(&mut m, UiKey::Home, false).await;
    assert_eq!(m.value, 0.);
    press(&mut m, UiKey::Left, false).await;
    assert_eq!(m.value, 0.);
    press(&mut m, UiKey::End, false).await;
    assert_eq!(m.value, 1.);
    press(&mut m, UiKey::Right, false).await;
    assert_eq!(m.value, 1.);
    press(&mut m, UiKey::Down, false).await;
    assert!((m.value - 0.9).abs() < 1e-6);
    press(&mut m, UiKey::Tab, false).await;
    assert_eq!(root_ui().focused_widget(), Some(CHECK));
    press(&mut m, UiKey::Space, false).await;
    assert!(m.checked);
    press(&mut m, UiKey::Tab, true).await;
    assert_eq!(root_ui().focused_widget(), Some(SLIDER));
    for step in [0., -1., f32::NAN, f32::INFINITY] {
        let before = m.value;
        m.step = step;
        root_ui().set_input_focus(SLIDER);
        press(&mut m, UiKey::Home, false).await;
        assert_eq!(m.value, before);
        assert_ne!(root_ui().focused_widget(), Some(SLIDER));
    }
    m.step = 0.1;
    for range in [1.0..1.0, 2.0..1.0, f32::NAN..1., 0.0..f32::INFINITY] {
        let before = m.value;
        m.range = range;
        root_ui().set_input_focus(SLIDER);
        press(&mut m, UiKey::End, false).await;
        assert_eq!(m.value, before);
    }
    m.range = 0.0..1.0;
    for value in [f32::NAN, f32::INFINITY] {
        m.value = value;
        root_ui().set_input_focus(SLIDER);
        press(&mut m, UiKey::Home, false).await;
        assert_eq!(m.value.to_bits(), value.to_bits());
    }
    m.value = 0.5;
    m.enabled = false;
    root_ui().set_input_focus(SLIDER);
    press(&mut m, UiKey::End, false).await;
    assert_eq!(m.value, 0.5);
    m.enabled = true;
    settle(&mut m).await;
    m.show = false;
    root_ui().set_input_focus(SLIDER);
    press(&mut m, UiKey::Right, false).await;
    assert_eq!(m.value, 0.5);
    assert_ne!(root_ui().focused_widget(), Some(SLIDER));
    m.show = true;
    m.clipped = true;
    root_ui().set_input_focus(SLIDER);
    press(&mut m, UiKey::End, false).await;
    assert_eq!(m.value, 0.5);
    m.clipped = false;
    settle(&mut m).await;
    root_ui().set_input_focus(OPEN);
    press(&mut m, UiKey::Enter, false).await;
    assert!(m.modal);
    press(&mut m, UiKey::Tab, false).await;
    assert_eq!(root_ui().focused_widget(), Some(MODAL_SLIDER));
    press(&mut m, UiKey::End, false).await;
    assert_eq!(m.modal_value, 1.);
    assert_eq!(m.value, 0.5);
    press(&mut m, UiKey::Tab, false).await;
    assert_eq!(root_ui().focused_widget(), Some(CLOSE));
    press(&mut m, UiKey::Enter, false).await;
    assert!(!m.modal);
    assert_eq!(root_ui().focused_widget(), Some(OPEN));
    // Numeric field remains editable, separately from track arrows.
    root_ui().set_input_focus(hash!(SLIDER, "editbox"));
    root_ui().key_down(UiKey::A, false, true);
    m.draw();
    next_frame().await;
    root_ui().key_up(UiKey::A);
    for ch in "0.25".chars() {
        root_ui().char_event(ch, false, false);
    }
    settle(&mut m).await;
    assert_eq!(m.value, 0.25);
    m.value = 0.5;
    root_ui().clear_input_focus();
    settle(&mut m).await;
    let rect = m.rect;
    drag(&mut m, rect).await;
    assert!((m.value - 0.75).abs() < 1e-5, "opt-in dragging still works");
    assert_eq!(root_ui().focused_widget(), Some(SLIDER));
    let rect = m.legacy_rect;
    drag(&mut m, rect).await;
    assert!(
        (m.legacy - 0.75).abs() < 1e-5,
        "legacy dragging still works"
    );
    assert_eq!(
        root_ui().focused_widget(),
        None,
        "legacy release clears drag focus"
    );
    m.enabled = false;
    m.value = 0.5;
    settle(&mut m).await;
    let rect = m.rect;
    drag(&mut m, rect).await;
    assert_eq!(m.value, 0.5, "disabled dragging preserves value");
    root_ui().set_input_focus(hash!(SLIDER, "editbox"));
    root_ui().char_event('9', false, false);
    settle(&mut m).await;
    assert_eq!(m.value, 0.5, "disabled numeric field preserves value");
    m.enabled = true;
    // Label/editbox reservations can exhaust compact layouts. An absent track
    // must neither register nor respond, but its numeric field still works.
    for width in [0., -4., f32::NEG_INFINITY, f32::NAN] {
        m.track_width = Some(width);
        m.value = 0.5;
        settle(&mut m).await;
        root_ui().set_input_focus(SLIDER);
        press(&mut m, UiKey::End, false).await;
        assert_eq!(m.value, 0.5, "unusable track must preserve value");
        assert_ne!(root_ui().focused_widget(), Some(SLIDER));
        root_ui().clear_input_focus();
        press(&mut m, UiKey::Tab, false).await;
        assert_eq!(root_ui().focused_widget(), Some(hash!(SLIDER, "editbox")));
        press(&mut m, UiKey::Tab, false).await;
        assert_eq!(root_ui().focused_widget(), Some(CHECK));
        let rect = Rect::new(m.rect.x, m.rect.y, 0., m.rect.h);
        drag(&mut m, rect).await;
        assert_eq!(m.value, 0.5, "unusable track ignores pointer dragging");
        root_ui().set_input_focus(hash!(SLIDER, "editbox"));
        root_ui().key_down(UiKey::A, false, true);
        m.draw();
        next_frame().await;
        root_ui().key_up(UiKey::A);
        for ch in "0.25".chars() {
            root_ui().char_event(ch, false, false);
        }
        settle(&mut m).await;
        assert_eq!(
            m.value, 0.25,
            "numeric editing survives exhausted track width"
        );
    }
    m.track_width = None;
    // Restore a predictable live menu after all automated assertions.
    m.value = 0.5;
    m.modal_value = 0.5;
    m.checked = false;
    root_ui().clear_input_focus();
    settle(&mut m).await;
    println!("KEYBOARD_SLIDERS_PASS");
    m
}
