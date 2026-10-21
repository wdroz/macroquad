use crate::{
    color::Color,
    math::{vec2, Rect, Vec2},
    ui::{ElementState, Id, Layout, Ui, UiContent},
};

pub struct Checkbox<'a> {
    id: Id,
    label: &'a str,
    ratio: f32,
    pos: Option<Vec2>,
    size: Option<Vec2>,
    keyboard_focus: bool,
    enabled: bool,
    focus_color: Color,
}

impl<'a> Checkbox<'a> {
    pub const fn new(id: Id) -> Checkbox<'a> {
        Checkbox {
            id,
            label: "",
            ratio: 0.5,
            pos: None,
            size: None,
            keyboard_focus: false,
            enabled: true,
            focus_color: Color::from_rgba(40, 110, 255, 255),
        }
    }

    pub const fn ratio(self, ratio: f32) -> Self {
        Self { ratio, ..self }
    }

    pub const fn label<'b>(self, label: &'b str) -> Checkbox<'b> {
        Checkbox {
            id: self.id,
            label,
            ratio: self.ratio,
            pos: self.pos,
            size: self.size,
            keyboard_focus: self.keyboard_focus,
            enabled: self.enabled,
            focus_color: self.focus_color,
        }
    }

    pub const fn pos(self, pos: Vec2) -> Self {
        Self {
            pos: Some(pos),
            ..self
        }
    }

    pub const fn size(self, size: Vec2) -> Self {
        Self {
            size: Some(size),
            ..self
        }
    }

    /// Opt into the shared editbox/button Tab and Shift+Tab traversal using
    /// this checkbox's existing ID. IDs must be unique across the `Ui`, including
    /// modals, and stable when labels or order change. Traversal is applied next
    /// frame; disabled and fully clipped controls are skipped.
    ///
    /// Space **and Enter** toggle once per press, ignoring held-key repeats.
    /// Modal containment and restoration follow buttons. Existing checkbox calls
    /// remain pointer-only, with their release-to-toggle behavior unchanged.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// use macroquad::ui::{hash, root_ui, widgets};
    /// let mut assist = false;
    /// widgets::Checkbox::new(hash!("step-assist"))
    ///     .keyboard_focus(true)
    ///     .enabled(true)
    ///     .focus_color(YELLOW)
    ///     .label("Step assist")
    ///     .ui(&mut root_ui(), &mut assist);
    /// ```
    pub const fn keyboard_focus(self, keyboard_focus: bool) -> Self {
        Self {
            keyboard_focus,
            ..self
        }
    }

    /// Disable pointer/keyboard toggles and omit the checkbox from traversal.
    /// Disabled checkboxes use the skin's inactive colors.
    pub const fn enabled(self, enabled: bool) -> Self {
        Self { enabled, ..self }
    }

    /// Color of the inset keyboard-focus outline, blue by default, independent
    /// of checked state and skin. Supply another visible indicator if transparent.
    pub const fn focus_color(self, focus_color: Color) -> Self {
        Self {
            focus_color,
            ..self
        }
    }

    pub fn ui(self, ui: &mut Ui, data: &mut bool) {
        let context = ui.get_active_window_context();

        let label_size = context.window.painter.content_with_margins_size(
            &context.style.label_style,
            &UiContent::Label(self.label.into()),
        );
        let size = self.size.unwrap_or(vec2(
            context
                .window
                .cursor
                .widget_width(context.style.margin * 2.),
            label_size.y.max(22.),
        ));

        let pos = self
            .pos
            .map(|pos| pos + context.window.cursor.fit(size, Layout::Vertical))
            .unwrap_or_else(|| context.window.cursor.fit(size, Layout::Vertical));

        let whole_area = Vec2::new(
            if self.label.is_empty() {
                size.x
            } else {
                size.x * self.ratio
            },
            size.y,
        );
        let checkbox_area = Vec2::new(19., 19.);
        let checkbox_pos = Vec2::new(
            pos.x + whole_area.x - 19. - 15.,
            pos.y + context.style.margin,
        );

        let rect = Rect::new(
            checkbox_pos.x,
            checkbox_pos.y,
            checkbox_area.x,
            checkbox_area.y,
        );
        let hovered = self.enabled
            && context.keyboard_focus.allows(context.keyboard_scope)
            && context.input.window_active
            && rect.contains(context.input.mouse_position)
            && context.window.painter.clipping_zone.map_or(true, |clip| {
                clip.w > 0. && clip.h > 0. && clip.contains(context.input.mouse_position)
            });
        let pointer_clicked = hovered && context.input.click_up();
        let keyboard_visible = context.keyboard_visible(rect);
        let keyboard_focused = if self.keyboard_focus {
            if pointer_clicked {
                *context.input_focus = Some(self.id);
            }
            context.keyboard_focus.register(
                self.id,
                context.keyboard_scope,
                self.enabled && context.keyboard_eligible && keyboard_visible,
                context.input_focus,
            )
        } else {
            false
        };
        let keyboard_clicked = context.keyboard_focus.activate(keyboard_focused);

        let background = context
            .style
            .checkbox_style
            .background_sprite(ElementState {
                focused: context.focused && self.enabled,
                hovered,
                clicked: *data,
                selected: false,
            });

        let color = context.style.checkbox_style.color(ElementState {
            focused: context.focused && self.enabled,
            hovered,
            clicked: hovered && context.input.is_mouse_down,
            selected: *data,
        });

        if let Some(background) = background {
            let background_margin = context
                .style
                .checkbox_style
                .background_margin
                .unwrap_or_default();

            context.window.painter.draw_sprite(
                Rect::new(checkbox_pos.x, checkbox_pos.y, 19., 19.),
                background,
                color,
                Some(background_margin),
            );
        } else {
            context.window.painter.draw_rect(
                Rect::new(
                    checkbox_pos.x,
                    checkbox_pos.y,
                    checkbox_area.x,
                    checkbox_area.y,
                ),
                None,
                color,
            );
        }

        if keyboard_focused {
            context.window.painter.draw_rect(
                Rect::new(checkbox_pos.x + 2., checkbox_pos.y + 2., 15., 15.),
                self.focus_color,
                None,
            );
        }

        if pointer_clicked || keyboard_clicked {
            *data ^= true;
        }

        let context = ui.get_active_window_context();

        if self.label.is_empty() == false {
            context.window.painter.draw_element_label(
                &context.style.label_style,
                Vec2::new(pos.x + size.x * self.ratio, pos.y),
                self.label,
                ElementState {
                    focused: context.focused && self.enabled,
                    hovered: false,
                    clicked: false,
                    selected: false,
                },
            );
        }
    }
}

impl Ui {
    pub fn checkbox(&mut self, id: Id, label: &str, data: &mut bool) {
        Checkbox::new(id).label(label).ui(self, data);
    }
}
