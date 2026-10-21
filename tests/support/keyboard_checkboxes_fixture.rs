//! Native and WebGL use the same menu and public input-interface checks.
use macroquad::{
    prelude::*,
    ui::{root_ui, widgets, InputHandler, KeyCode as UiKey, Layout, LayoutSettings, Skin},
};

pub const APPLY: u64 = 101;
pub const EDIT: u64 = 102;
pub const DISABLED: u64 = 103;
pub const EXTRA: u64 = 104;
pub const OPEN: u64 = 105;
pub const KEEP: u64 = 106;
pub const CLOSE: u64 = 107;
const MODAL_EDIT: u64 = 108;

pub struct Menu {
    pub apply: u32,
    pub extra: u32,
    pub keep: u32,
    pub legacy: u32,
    pub disabled_clicks: u32,
    pub modal: bool,
    pub show_extra: bool,
    pub apply_enabled: bool,
    keep_enabled: bool,
    pub text: String,
    modal_text: String,
    modal_edit_mode: bool,
    pub rectangles: [Rect; 8],
    skin: Skin,
    values: [bool; 5],
}

impl Menu {
    pub fn new() -> Self {
        let ui = root_ui();
        let skin = Skin {
            margin: 0.,
            window_style: ui
                .style_builder()
                .color(Color::from_rgba(35, 46, 66, 255))
                .build(),
            label_style: ui.style_builder().font_size(17).text_color(WHITE).build(),
            button_style: ui
                .style_builder()
                .font_size(17)
                .color(Color::from_rgba(225, 232, 240, 255))
                .color_hovered(Color::from_rgba(246, 249, 252, 255))
                .color_clicked(Color::from_rgba(170, 195, 220, 255))
                .build(),
            ..ui.default_skin()
        };
        Self {
            apply: 0,
            extra: 0,
            keep: 0,
            legacy: 0,
            disabled_clicks: 0,
            modal: false,
            show_extra: true,
            apply_enabled: true,
            keep_enabled: true,
            text: String::new(),
            modal_text: String::new(),
            modal_edit_mode: false,
            rectangles: [Rect::default(); 8],
            skin,
            values: [false; 5],
        }
    }

    pub fn draw(&mut self) {
        set_default_camera();
        clear_background(Color::from_rgba(18, 27, 42, 255));
        draw_text("Tab / Shift+Tab / Enter / Space", 12., 28., 21., WHITE);
        let width = (screen_width() - 24.).clamp(1., 520.);
        let mut ui = root_ui();
        ui.push_skin(&self.skin);
        widgets::Window::new(900, vec2(12., 44.), vec2(width, 294.))
            .movable(false)
            .titlebar(false)
            .ui(&mut ui, |ui| {
                ui.set_layout(
                    LayoutSettings::new(Layout::Vertical)
                        .padding(RectOffset::new(10., 10., 10., 10.))
                        .gap(vec2(0., 8.)),
                );
                let width = ui.available_width();
                let mut control =
                    |ui: &mut macroquad::ui::Ui, rect: &mut Rect, id, label, enabled| {
                        let p = ui.canvas().cursor();
                        *rect = Rect::new(p.x, p.y, width, 32.);
                        if id == OPEN {
                            widgets::Button::new(label)
                                .id(id)
                                .enabled(enabled)
                                .focus_color(YELLOW)
                                .size(vec2(width, 32.))
                                .ui(ui)
                        } else {
                            *rect = Rect::new(p.x, p.y, 19., 19.);
                            let slot = if id == APPLY {
                                0
                            } else if id == DISABLED {
                                4
                            } else {
                                1
                            };
                            let before = self.values[slot];
                            widgets::Checkbox::new(id)
                                .keyboard_focus(true)
                                .enabled(enabled)
                                .focus_color(YELLOW)
                                .label(label)
                                .size(vec2(width, 32.))
                                .ratio(34. / width)
                                .ui(ui, &mut self.values[slot]);
                            self.values[slot] != before
                        }
                    };
                if control(
                    ui,
                    &mut self.rectangles[0],
                    APPLY,
                    "Apply",
                    self.apply_enabled,
                ) {
                    self.apply += 1;
                }
                let p = ui.canvas().cursor();
                self.rectangles[1] = Rect::new(p.x, p.y, width, 32.);
                widgets::Editbox::new(EDIT, vec2(width, 32.))
                    .multiline(false)
                    .ui(ui, &mut self.text);
                if control(
                    ui,
                    &mut self.rectangles[2],
                    DISABLED,
                    "Disabled action",
                    false,
                ) {
                    self.disabled_clicks += 1;
                }
                if self.show_extra
                    && control(ui, &mut self.rectangles[3], EXTRA, "Temporary action", true)
                {
                    self.extra += 1;
                }
                if control(ui, &mut self.rectangles[4], OPEN, "Open confirmation", true) {
                    self.modal = true;
                }
                let p = ui.canvas().cursor();
                self.rectangles[7] = Rect::new(p.x, p.y, 19., 19.);
                let before = self.values[3];
                widgets::Checkbox::new(109)
                    .label("Legacy pointer checkbox")
                    .size(vec2(width, 32.))
                    .ratio(34. / width)
                    .ui(ui, &mut self.values[3]);
                if self.values[3] != before {
                    self.legacy += 1;
                }
            });
        if self.modal {
            let width = (screen_width() - 72.).clamp(1., 330.);
            let height = if self.modal_edit_mode { 204. } else { 164. };
            ui.modal(901, vec2(36., 100.), vec2(width, height), |ui| {
                ui.canvas().rect(
                    Rect::new(36., 100., width, height),
                    WHITE,
                    Color::from_rgba(45, 62, 89, 255),
                );
                ui.set_layout(
                    LayoutSettings::new(Layout::Vertical)
                        .padding(RectOffset::new(12., 12., 12., 12.))
                        .gap(vec2(0., 10.)),
                );
                widgets::Label::new("Modal: focus stays inside")
                    .size(vec2(ui.available_width(), 24.))
                    .ui(ui);
                let width = ui.available_width();
                widgets::Group::new(902, vec2(width - 8., height - 90.))
                    .layout(Layout::Vertical)
                    .content_layout(LayoutSettings::new(Layout::Vertical).gap(vec2(0., 10.)))
                    .ui(ui, |ui| {
                        assert!(
                            ui.available_width() > 0. && ui.available_width() <= width - 8. + 0.01,
                            "nested modal Group must use its own content viewport"
                        );
                        let width = ui.available_width();
                        if self.modal_edit_mode {
                            widgets::Editbox::new(MODAL_EDIT, vec2(width, 30.))
                                .multiline(false)
                                .ui(ui, &mut self.modal_text);
                        }
                        for (slot, id, label) in [
                            (5, KEEP, "Confirm (stay open)"),
                            (6, CLOSE, "Close confirmation"),
                        ] {
                            let p = ui.canvas().cursor();
                            self.rectangles[slot] = Rect::new(p.x, p.y, width, 32.);
                            let activated = if id == KEEP {
                                self.rectangles[slot] = Rect::new(p.x, p.y, 19., 19.);
                                let before = self.values[2];
                                widgets::Checkbox::new(id)
                                    .keyboard_focus(true)
                                    .enabled(self.keep_enabled)
                                    .focus_color(YELLOW)
                                    .label(label)
                                    .size(vec2(width, 32.))
                                    .ratio(34. / width)
                                    .ui(ui, &mut self.values[2]);
                                self.values[2] != before
                            } else {
                                widgets::Button::new(label)
                                    .id(id)
                                    .focus_color(YELLOW)
                                    .size(vec2(width, 32.))
                                    .ui(ui)
                            };
                            if activated {
                                if id == KEEP {
                                    self.keep += 1;
                                } else {
                                    self.modal = false;
                                }
                            }
                        }
                    });
            });
        }
        ui.pop_skin();
        drop(ui);
        draw_text(
            format!(
                "apply={} temporary={} confirm={} legacy={}",
                self.apply, self.extra, self.keep, self.legacy
            ),
            12.,
            screen_height() - 18.,
            16.,
            WHITE,
        );
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

fn focus() -> Option<u64> {
    root_ui().focused_widget()
}

pub async fn run() -> Menu {
    let mut menu = Menu::new();
    settle(&mut menu).await;
    root_ui().clear_input_focus();
    press(&mut menu, UiKey::Tab, false).await;
    assert_eq!(
        focus(),
        Some(APPLY),
        "first Tab selects first enabled stable control"
    );
    press(&mut menu, UiKey::Enter, false).await;
    assert_eq!(menu.apply, 1);
    press(&mut menu, UiKey::Tab, false).await;
    assert_eq!(focus(), Some(EDIT), "editboxes share the traversal order");
    root_ui().char_event('q', false, false);
    settle(&mut menu).await;
    assert_eq!(menu.text, "q");
    press(&mut menu, UiKey::Tab, false).await;
    assert_eq!(focus(), Some(EXTRA), "disabled controls are skipped");
    press(&mut menu, UiKey::Tab, true).await;
    assert_eq!(focus(), Some(EDIT));
    press(&mut menu, UiKey::Tab, true).await;
    assert_eq!(focus(), Some(APPLY));
    let before = menu.apply;
    root_ui().key_down(UiKey::Space, false, false);
    settle(&mut menu).await;
    for _ in 0..4 {
        root_ui().key_down(UiKey::Space, false, false);
        menu.draw();
        next_frame().await;
    }
    assert_eq!(
        menu.apply,
        before + 1,
        "held/repeated Space activates exactly once"
    );
    root_ui().key_up(UiKey::Space);
    press(&mut menu, UiKey::Space, false).await;
    assert_eq!(menu.apply, before + 2, "release enables a new activation");

    root_ui().set_input_focus(EXTRA);
    menu.show_extra = false;
    press(&mut menu, UiKey::Enter, false).await;
    assert_ne!(
        focus(),
        Some(EXTRA),
        "a removed control cannot retain focus"
    );
    assert_eq!(menu.extra, 0);
    menu.show_extra = true;
    settle(&mut menu).await;
    root_ui().set_input_focus(APPLY);
    menu.apply_enabled = false;
    press(&mut menu, UiKey::Enter, false).await;
    assert_ne!(
        focus(),
        Some(APPLY),
        "a disabled control cannot retain focus"
    );
    assert_eq!(menu.apply, before + 2);
    menu.apply_enabled = true;
    settle(&mut menu).await;

    root_ui().set_input_focus(OPEN);
    press(&mut menu, UiKey::Enter, false).await;
    assert!(menu.modal);
    assert_eq!(focus(), Some(KEEP));
    assert_eq!(
        menu.keep, 0,
        "opening activation must not pass into the modal"
    );
    let apply_before_modal = menu.apply;
    let text_before_modal = menu.text.clone();
    for slot in [0, 1] {
        let rect = menu.rectangles[slot];
        // The left edge remains outside the popup and inside its background control.
        let point = (rect.x + 2., rect.y + rect.h * 0.5);
        root_ui().mouse_move(point);
        root_ui().mouse_down(point);
        menu.draw();
        next_frame().await;
        root_ui().mouse_move(point);
        root_ui().mouse_up(point);
        root_ui().char_event('x', false, false);
        settle(&mut menu).await;
        assert_eq!(
            focus(),
            Some(KEEP),
            "background pointer input cannot steal modal focus"
        );
    }
    assert_eq!(menu.apply, apply_before_modal);
    assert_eq!(
        menu.text, text_before_modal,
        "background editbox cannot consume typing"
    );
    press(&mut menu, UiKey::Tab, true).await;
    assert_eq!(focus(), Some(CLOSE), "reverse traversal wraps inside modal");
    press(&mut menu, UiKey::Tab, false).await;
    assert_eq!(focus(), Some(KEEP));
    press(&mut menu, UiKey::Space, false).await;
    assert_eq!(menu.keep, 1);
    press(&mut menu, UiKey::Tab, false).await;
    press(&mut menu, UiKey::Enter, false).await;
    assert!(!menu.modal);
    assert_eq!(focus(), Some(OPEN), "closing restores the opener's focus");
    menu.modal_edit_mode = true;
    root_ui().key_down(UiKey::Space, false, false);
    root_ui().char_event(' ', false, false);
    settle(&mut menu).await;
    assert_eq!(focus(), Some(MODAL_EDIT));
    assert_eq!(
        menu.modal_text, "",
        "Space opening a modal cannot type into its first editbox"
    );
    root_ui().key_down(UiKey::Space, false, false);
    root_ui().char_event(' ', false, false);
    settle(&mut menu).await;
    assert_eq!(
        menu.modal_text, "",
        "holding the opener cannot type repeated spaces into the modal"
    );
    root_ui().key_up(UiKey::Space);
    root_ui().char_event('a', false, false);
    settle(&mut menu).await;
    assert_eq!(
        menu.modal_text, "a",
        "the modal editbox accepts fresh typing after release"
    );
    press(&mut menu, UiKey::Tab, false).await;
    assert_eq!(
        focus(),
        Some(KEEP),
        "modal Group shares the editbox/control traversal scope"
    );
    press(&mut menu, UiKey::Tab, false).await;
    press(&mut menu, UiKey::Enter, false).await;
    assert!(!menu.modal);
    assert_eq!(focus(), Some(OPEN));
    menu.modal_edit_mode = false;
    press(&mut menu, UiKey::Tab, false).await;
    assert_eq!(
        focus(),
        Some(APPLY),
        "legacy pointer-only checkbox calls do not enter the tab order"
    );
    let r = menu.rectangles[7];
    let point = (r.x + r.w * 0.5, r.y + r.h * 0.5);
    root_ui().mouse_move(point);
    root_ui().mouse_down(point);
    menu.draw();
    next_frame().await;
    root_ui().mouse_move(point);
    root_ui().mouse_up(point);
    settle(&mut menu).await;
    assert_eq!(menu.legacy, 1, "legacy pointer behavior stays available");
    assert_eq!(menu.disabled_clicks, 0);
    assert!(!menu.values[4]);
    root_ui().mouse_move((-100., -100.));
    menu.modal = true;
    settle(&mut menu).await;
    root_ui().set_input_focus(KEEP);
    menu.keep_enabled = false;
    press(&mut menu, UiKey::Enter, false).await;
    assert!(
        menu.modal,
        "disabling Confirm must not transfer its Enter activation to Close"
    );
    assert_eq!(focus(), Some(CLOSE));
    press(&mut menu, UiKey::Enter, false).await;
    assert!(
        !menu.modal,
        "a fresh Enter may activate the newly focused Close"
    );
    menu.keep_enabled = true;
    root_ui().set_input_focus(APPLY);
    settle(&mut menu).await;
    let r = menu.rectangles[0];
    let point = (r.x + r.w * 0.5, r.y + r.h * 0.5);
    root_ui().mouse_move(point);
    root_ui().mouse_down(point);
    menu.draw();
    next_frame().await;
    let before = menu.values[0];
    let count = menu.apply;
    root_ui().mouse_move(point);
    root_ui().mouse_up(point);
    root_ui().key_down(UiKey::Space, false, false);
    settle(&mut menu).await;
    assert_eq!(
        menu.values[0], !before,
        "pointer release plus Space toggles once"
    );
    assert_eq!(menu.apply, count + 1);
    assert_eq!(
        focus(),
        Some(APPLY),
        "pointer release acquires stable checkbox focus"
    );
    root_ui().key_up(UiKey::Space);
    root_ui().mouse_move((-100., -100.));
    settle(&mut menu).await;
    menu
}
