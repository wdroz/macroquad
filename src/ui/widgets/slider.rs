use crate::{
    color::Color,
    math::{vec2, Rect},
    ui::focus::SliderAction,
    ui::{widgets::Editbox, ElementState, Id, Layout, Ui},
};

use std::ops::Range;

pub struct Slider<'a> {
    id: Id,
    label: &'a str,
    range: Range<f32>,
    label_width: Option<f32>,
    keyboard_focus: bool,
    keyboard_step: f32,
    enabled: bool,
    focus_color: Color,
}

impl<'a> Slider<'a> {
    pub const fn new(id: Id, range: Range<f32>) -> Slider<'a> {
        Slider {
            id,
            range,
            label: "",
            label_width: None,
            keyboard_focus: false,
            keyboard_step: 0.,
            enabled: true,
            focus_color: Color::from_rgba(40, 110, 255, 255),
        }
    }

    pub const fn label<'b>(self, label: &'b str) -> Slider<'b> {
        Slider { label, ..self }
    }

    pub const fn label_width(self, width: f32) -> Self {
        Slider {
            label_width: Some(width),
            ..self
        }
    }

    /// Opt into shared Tab/Shift+Tab traversal with this stable, unique ID.
    /// The numeric editbox remains a separate preceding Tab stop. Configure a
    /// finite positive [`Self::keyboard_step`] as well. Right/Up increase and
    /// Left/Down decrease once per press; held keys do not repeat. Home/End set
    /// exact endpoints (the normally exclusive range end is included).
    /// Opposing arrows cancel; same-direction arrows count once. End wins over
    /// Home, which wins over arrows in the same frame. Ctrl combinations do
    /// nothing. Modal containment/restoration follow buttons and checkboxes.
    ///
    /// Values clamp to the range before stepping, with f64 intermediates then
    /// f32 rounding; steps smaller than f32 resolution may leave values unchanged.
    /// Invalid/nonascending ranges, nonfinite data, or an invalid step while
    /// opted in disable all input and preserve caller data. Legacy calls retain
    /// their pointer dragging and numeric editing behavior. If layout leaves no
    /// finite positive track width, track input and its Tab stop are skipped;
    /// the numeric field remains usable when otherwise enabled.
    ///
    /// ```no_run
    /// use macroquad::{prelude::*, ui::{hash, root_ui, widgets}};
    /// let mut volume = 0.5;
    /// widgets::Slider::new(hash!("volume"), 0.0..1.0)
    ///     .keyboard_focus(true).keyboard_step(0.1)
    ///     .label("Volume").ui(&mut root_ui(), &mut volume);
    /// ```
    pub const fn keyboard_focus(self, keyboard_focus: bool) -> Self {
        Self {
            keyboard_focus,
            ..self
        }
    }

    /// Caller-selected finite positive increment in value units; no default step.
    /// This builder can be configured without a window; call `ui` each frame
    /// inside a running macroquad application to submit the control.
    ///
    /// ```
    /// use macroquad::{prelude::YELLOW, ui::{hash, widgets::Slider}};
    /// let _volume = Slider::new(hash!("volume"), 0.0..1.0)
    ///     .keyboard_focus(true)
    ///     .keyboard_step(0.05)
    ///     .enabled(true)
    ///     .focus_color(YELLOW)
    ///     .label("Volume");
    /// ```
    pub const fn keyboard_step(self, keyboard_step: f32) -> Self {
        Self {
            keyboard_step,
            ..self
        }
    }

    /// Disable dragging, numeric editing and both Tab stops, preserving the value.
    pub const fn enabled(self, enabled: bool) -> Self {
        Self { enabled, ..self }
    }

    /// Inset track focus outline color, blue by default, independent of the skin.
    pub const fn focus_color(self, focus_color: Color) -> Self {
        Self {
            focus_color,
            ..self
        }
    }

    pub fn ui(self, ui: &mut Ui, data: &mut f32) {
        let valid = valid_range(&self.range, *data);
        let enabled = self.enabled
            && (!self.keyboard_focus
                || (valid && self.keyboard_step.is_finite() && self.keyboard_step > 0.));
        let context = ui.get_active_window_context();

        let size = vec2(
            context
                .window
                .cursor
                .widget_width(context.style.margin * 3.),
            19.,
        );
        let pos = context.window.cursor.fit(size, Layout::Vertical);

        let editbox_width = 50.;

        let label_width = self.label_width.unwrap_or(100.);
        let slider_width = size.x - editbox_width - label_width;
        let track_available = slider_width.is_finite() && slider_width > 0.;
        // The handle's hit padding does not make an exhausted track usable.
        // Keep drawing coordinates finite and avoid dividing by zero/reversing
        // the pointer mapping when label/editbox reservations consume the row.
        let slider_width = if track_available { slider_width } else { 0. };
        let margin = 5.;

        let mut temp_string = context
            .storage_any
            .get_or_insert_with::<String, _>(self.id, || format!("{:.2}", *data))
            .clone();

        let editbox_id = hash!(self.id, "editbox");
        if context.input_focused(editbox_id) == false {
            use std::fmt::Write;

            temp_string.clear();
            let _ = write!(&mut temp_string, "{:.2}", *data);
        }

        Editbox::new(editbox_id, vec2(50., size.y))
            .position(pos)
            .multiline(false)
            .filter(&|character| character.is_digit(10) || character == '.' || character == '-')
            .ui_enabled(ui, &mut temp_string, enabled);

        let context = ui.get_active_window_context();
        let old_string = context.storage_any.get_or_default::<String>(self.id);
        if enabled && *old_string != temp_string {
            if let Ok(num) = temp_string.parse::<f32>() {
                if num > self.range.end {
                    *data = self.range.end;
                } else if num < self.range.start {
                    *data = self.range.start;
                } else {
                    *data = num;
                }
            }
        }

        let slider_start_x = editbox_width + pos.x + margin;
        let track_rect = Rect::new(slider_start_x - 4., pos.y, slider_width + 8., 20.);
        let visible = track_available && context.keyboard_visible(track_rect);
        let interaction_allowed = enabled
            && context.keyboard_eligible
            && visible
            && context.keyboard_focus.allows(context.keyboard_scope);
        let pointer_allowed = interaction_allowed && context.input.window_active;
        let keyboard_focused = self.keyboard_focus
            && context.keyboard_focus.register(
                self.id,
                context.keyboard_scope,
                enabled && context.keyboard_eligible && visible,
                context.input_focus,
            );
        if let Some(action) = context.keyboard_focus.slider_action(keyboard_focused) {
            if let Some(value) = keyboard_value(&self.range, *data, self.keyboard_step, action) {
                *data = value;
                temp_string = format!("{:.2}", value);
            }
        }
        let old_string = context.storage_any.get_or_default::<String>(self.id);
        let dragging = context
            .storage_u32
            .entry(hash!(self.id, "dragging"))
            .or_insert(0);

        let fraction = if valid {
            ((*data as f64 - self.range.start as f64)
                / (self.range.end as f64 - self.range.start as f64)) as f32
        } else {
            0.
        };
        let data_pos = fraction * slider_width + slider_start_x;

        let bar_rect = Rect::new(data_pos - 4., pos.y, 8., 20.);
        let hovered = pointer_allowed
            && bar_rect.contains(context.input.mouse_position)
            && context
                .window
                .painter
                .clipping_zone
                .map_or(true, |clip| clip.contains(context.input.mouse_position));

        if hovered && context.input.is_mouse_down() {
            *dragging = 1;
            *context.input_focus = Some(self.id);
            context.input.cursor_grabbed = true;
        }

        if *dragging == 1 && (!interaction_allowed || context.input.is_mouse_down == false) {
            context.input.cursor_grabbed = false;
            *dragging = 0;
            if !self.keyboard_focus || !interaction_allowed {
                *context.input_focus = None;
            }
        }

        if *dragging == 1 {
            let mouse_position = ((context.input.mouse_position.x - slider_start_x) / slider_width)
                .min(1.)
                .max(0.);
            let old_data = *data;
            *data = if self.keyboard_focus {
                (self.range.start as f64
                    + (self.range.end as f64 - self.range.start as f64) * mouse_position as f64)
                    as f32
            } else {
                self.range.start + (self.range.end - self.range.start) * mouse_position
            };

            if old_data != *data {
                use std::fmt::Write;

                temp_string.clear();
                let _ = write!(&mut temp_string, "{:.2}", *data);
            }
        }

        if keyboard_focused {
            context.window.painter.draw_rect(
                Rect::new(
                    track_rect.x + 1.,
                    track_rect.y + 1.,
                    track_rect.w - 2.,
                    track_rect.h - 2.,
                ),
                self.focus_color,
                None,
            );
        }

        // static horizontal line in the middle of the slider
        context.window.painter.draw_line(
            vec2(pos.x + editbox_width + margin, pos.y + size.y / 2.),
            vec2(
                pos.x + editbox_width + slider_width + margin,
                pos.y + size.y / 2.,
            ),
            // TODO: introduce separate slider_style
            context.style.checkbox_style.color(ElementState {
                focused: context.focused,
                hovered: true,
                clicked: true,
                selected: false,
            }),
        );

        // slider bar
        context.window.painter.draw_rect(
            bar_rect,
            None,
            context.style.checkbox_style.color(ElementState {
                focused: context.focused,
                hovered,
                clicked: hovered && context.input.is_mouse_down,
                selected: false,
            }),
        );

        context.window.painter.draw_element_label(
            &context.style.label_style,
            vec2(
                pos.x + editbox_width + slider_width + margin * 2.,
                pos.y + 2.,
            ),
            self.label,
            ElementState {
                focused: context.focused,
                ..Default::default()
            },
        );

        *old_string = temp_string;
    }
}

impl Ui {
    pub fn slider(&mut self, id: Id, label: &str, range: Range<f32>, data: &mut f32) {
        Slider::new(id, range).label(label).ui(self, data);
    }
}

fn valid_range(range: &Range<f32>, value: f32) -> bool {
    range.start.is_finite() && range.end.is_finite() && range.start < range.end && value.is_finite()
}

fn keyboard_value(range: &Range<f32>, value: f32, step: f32, action: SliderAction) -> Option<f32> {
    if !valid_range(range, value) || !step.is_finite() || step <= 0. {
        return None;
    }
    Some(match action {
        SliderAction::Start => range.start,
        SliderAction::End => range.end,
        SliderAction::Step(direction) => {
            let start = range.start as f64;
            let end = range.end as f64;
            ((value as f64).clamp(start, end) + step as f64 * direction as f64).clamp(start, end)
                as f32
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_steps_round_clamp_and_preserve_endpoints() {
        let step = SliderAction::Step(1);
        let mut value = 0.;
        for _ in 0..11 {
            value = keyboard_value(&(0.0..1.0), value, 0.1, step).unwrap();
        }
        assert_eq!(value, 1.);
        assert_eq!(keyboard_value(&(0.0..1.0), -10., 0.25, step), Some(0.25));
        assert_eq!(
            keyboard_value(&(-f32::MAX..f32::MAX), 0., f32::MAX, step),
            Some(f32::MAX)
        );
        assert_eq!(
            keyboard_value(&(0.0..1.0), 0.7, 0.3, SliderAction::Start),
            Some(0.)
        );
        assert_eq!(
            keyboard_value(&(0.0..1.0), 0.7, 0.3, SliderAction::End),
            Some(1.)
        );
        assert_eq!(
            keyboard_value(&(0.0..1.0), 1., f32::MIN_POSITIVE, SliderAction::Step(-1)),
            Some(1.)
        );
    }

    #[test]
    fn invalid_input_has_no_result_even_for_endpoints() {
        for range in [0.0..0.0, 1.0..0.0, f32::NAN..1., 0.0..f32::INFINITY] {
            assert_eq!(keyboard_value(&range, 0., 0.1, SliderAction::End), None);
        }
        for step in [0., -1., f32::NAN, f32::INFINITY] {
            assert_eq!(
                keyboard_value(&(0.0..1.0), 0.5, step, SliderAction::End),
                None
            );
        }
        for value in [f32::NAN, f32::NEG_INFINITY, f32::INFINITY] {
            assert_eq!(
                keyboard_value(&(0.0..1.0), value, 0.1, SliderAction::Start),
                None
            );
        }
    }
}
