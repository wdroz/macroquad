use crate::{
    color::Color,
    math::{Rect, Vec2},
    ui::{ElementState, Id, Layout, Ui, UiContent},
};

pub struct Button<'a> {
    position: Option<Vec2>,
    size: Option<Vec2>,
    content: UiContent<'a>,
    selected: bool,
    id: Option<Id>,
    enabled: bool,
    focus_color: Color,
}

impl<'a> Button<'a> {
    pub fn new<S>(content: S) -> Button<'a>
    where
        S: Into<UiContent<'a>>,
    {
        Button {
            position: None,
            size: None,
            content: content.into(),
            selected: false,
            id: None,
            enabled: true,
            focus_color: Color::from_rgba(40, 110, 255, 255),
        }
    }

    pub fn position<P: Into<Option<Vec2>>>(self, position: P) -> Self {
        let position = position.into();

        Button { position, ..self }
    }

    pub fn size(self, size: Vec2) -> Self {
        Button {
            size: Some(size),
            ..self
        }
    }

    pub fn selected(self, selected: bool) -> Self {
        Button { selected, ..self }
    }

    /// Opt into shared editbox/button keyboard traversal with a stable ID.
    ///
    /// IDs must be unique across the `Ui`, including popup contents, and stay
    /// the same when labels or ordering change. Tab/Shift+Tab traverse enabled
    /// visible controls in submission order, applying the result next frame.
    /// Fully clipped controls are skipped; scroll to reveal them first. Enter and
    /// Space activate once per press. Buttons without an ID remain pointer-only.
    pub fn id(self, id: Id) -> Self {
        Self {
            id: Some(id),
            ..self
        }
    }

    /// Disable pointer and keyboard activation and remove this button from
    /// traversal. Disabled buttons use the skin's inactive colors.
    pub fn enabled(self, enabled: bool) -> Self {
        Self { enabled, ..self }
    }

    /// Color of the inset keyboard-focus outline, blue by default. This outline
    /// is independent of selection and skin colors. Use transparent only when
    /// supplying a separate visible focus indicator.
    pub fn focus_color(self, focus_color: Color) -> Self {
        Self {
            focus_color,
            ..self
        }
    }

    pub fn ui(self, ui: &mut Ui) -> bool {
        let mut context = ui.get_active_window_context();

        let size = self.size.unwrap_or_else(|| {
            context
                .window
                .painter
                .content_with_margins_size(&context.style.button_style, &self.content)
        });

        let pos = context
            .window
            .cursor
            .fit(size, self.position.map_or(Layout::Vertical, Layout::Free));
        let rect = Rect::new(pos.x, pos.y, size.x, size.y);
        let (hovered, pointer_clicked) = if self.enabled {
            context.register_click_intention(rect)
        } else {
            *context.last_item_clicked = false;
            *context.last_item_hovered = false;
            (false, false)
        };
        let keyboard_visible = context.keyboard_visible(rect);
        let keyboard_focused = if let Some(id) = self.id {
            if pointer_clicked {
                *context.input_focus = Some(id);
            }
            context.keyboard_focus.register(
                id,
                context.keyboard_scope,
                self.enabled && context.keyboard_eligible && keyboard_visible,
                context.input_focus,
            )
        } else {
            false
        };
        let activated = context.keyboard_focus.activate(keyboard_focused);
        let clicked = pointer_clicked || activated;
        *context.last_item_clicked = clicked;

        if !context.style.button_style.reverse_background_z {
            context.window.painter.draw_element_background(
                &context.style.button_style,
                pos,
                size,
                ElementState {
                    focused: context.focused && self.enabled,
                    hovered,
                    clicked: hovered && context.input.is_mouse_down,
                    selected: self.selected,
                },
            );
        }

        context.window.painter.draw_element_content(
            &context.style.button_style,
            pos,
            size,
            &self.content,
            ElementState {
                focused: context.focused && self.enabled,
                hovered,
                clicked: hovered && context.input.is_mouse_down,
                selected: self.selected,
            },
        );

        if context.style.button_style.reverse_background_z {
            context.window.painter.draw_element_background(
                &context.style.button_style,
                pos,
                size,
                ElementState {
                    focused: context.focused && self.enabled,
                    hovered,
                    clicked: hovered && context.input.is_mouse_down,
                    selected: false,
                },
            );
        }

        if keyboard_focused && size.x > 4. && size.y > 4. {
            context.window.painter.draw_rect(
                Rect::new(pos.x + 2., pos.y + 2., size.x - 4., size.y - 4.),
                self.focus_color,
                None,
            );
        }

        clicked
    }
}

impl Ui {
    pub fn button<'a, P: Into<Option<Vec2>>, S: Into<UiContent<'a>>>(
        &mut self,
        position: P,
        label: S,
    ) -> bool {
        Button::new(label).position(position).ui(self)
    }
}
