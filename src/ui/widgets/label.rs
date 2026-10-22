use crate::{
    math::{vec2, Rect, RectOffset, Vec2},
    text::{wrap_text, TextDimensions},
    ui::{ElementState, Layout, Ui, UiContent},
};

use std::borrow::Cow;

#[derive(Debug)]
struct WrappedLabel {
    size: Vec2,
    lines: Vec<String>,
    left: f32,
    top: f32,
    content_width: f32,
    content_height: f32,
    baseline: f32,
    spacing: f32,
}

fn nonnegative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.)
    } else {
        0.
    }
}

fn finite_sum(a: f32, b: f32) -> f32 {
    (a as f64 + b as f64).min(f32::MAX as f64) as f32
}

// Pure allocation policy; the live path supplies the existing wrapping and font
// measurement functions. Empty lines bypass measure_text's empty-ink sentinel.
#[allow(clippy::too_many_arguments)]
fn wrapped_layout(
    text: &str,
    width: f32,
    margin: RectOffset,
    font_size: f32,
    ascent: f32,
    spacing: Option<f32>,
    minimum_height: f32,
    mut wrap: impl FnMut(&str, f32) -> String,
    mut measure: impl FnMut(&str) -> TextDimensions,
) -> WrappedLabel {
    let width = nonnegative(width);
    let left = nonnegative(margin.left);
    let right = nonnegative(margin.right);
    let top = nonnegative(margin.top);
    let bottom = nonnegative(margin.bottom);
    let font_size = nonnegative(font_size);
    let content_width = (width - left - right).max(0.);
    let spacing = spacing
        .filter(|value| value.is_finite() && *value > 0.)
        .unwrap_or(font_size);
    let mut layout = WrappedLabel {
        size: Vec2::ZERO,
        lines: Vec::new(),
        left,
        top,
        content_width,
        content_height: font_size,
        baseline: nonnegative(ascent),
        spacing,
    };
    if width == 0. {
        return layout;
    }
    if content_width == 0. || font_size == 0. {
        layout.lines.push(String::new());
    } else {
        for paragraph in text.split('\n') {
            let paragraph = paragraph.strip_suffix('\r').unwrap_or(paragraph);
            let wrapped = wrap(paragraph, content_width);
            // Hard breaks were already handled above. wrap_text can insert
            // multiple synthetic empty lines around an overwide glyph; those
            // do not represent additional hard breaks in this paragraph.
            let start = layout.lines.len();
            layout.lines.extend(
                wrapped
                    .split('\n')
                    .filter(|line| !line.is_empty())
                    .map(str::to_owned),
            );
            if layout.lines.len() == start {
                layout.lines.push(String::new());
            }
        }
    }
    let dimensions: Vec<_> = layout
        .lines
        .iter()
        .map(|line| {
            if line.is_empty() {
                TextDimensions::default()
            } else {
                measure(line)
            }
        })
        .collect();
    for dimensions in &dimensions {
        layout.baseline = layout.baseline.max(nonnegative(dimensions.offset_y));
    }
    for (index, dimensions) in dimensions.iter().enumerate() {
        let offset = (index as f64 * spacing as f64).min(f32::MAX as f64) as f32;
        layout.content_height = layout.content_height.max(finite_sum(offset, font_size));
        let ink_bottom = finite_sum(
            layout.baseline,
            (dimensions.height - dimensions.offset_y).max(0.),
        );
        layout.content_height = layout.content_height.max(finite_sum(offset, ink_bottom));
    }
    layout.size = vec2(
        width,
        finite_sum(finite_sum(top, layout.content_height), bottom).max(nonnegative(minimum_height)),
    );
    layout
}

pub struct Label<'a> {
    position: Option<Vec2>,
    _multiline: Option<f32>,
    wrap: Option<f32>,
    size: Option<Vec2>,
    label: Cow<'a, str>,
}

impl<'a> Label<'a> {
    pub fn new<S>(label: S) -> Label<'a>
    where
        S: Into<Cow<'a, str>>,
    {
        Label {
            position: None,
            _multiline: None,
            wrap: None,
            size: None,
            label: label.into(),
        }
    }

    /// Baseline spacing in logical window units for [`Self::wrap`]. Invalid or
    /// nonpositive spacing uses the skin's font size. Smaller positive values
    /// intentionally overlap lines; measured height still contains their ink.
    /// Without `wrap`, this retains its historical no-op behavior.
    pub fn multiline(self, line_height: f32) -> Self {
        Label {
            _multiline: Some(line_height),
            ..self
        }
    }

    /// Opt into wrapping within an outer width in logical window units.
    ///
    /// The width includes skin margins and background margins. Their combined
    /// sides are clamped to finite nonnegative values. Lines start at the inner
    /// left edge; `\n` and CRLF are hard breaks, including trailing empty lines.
    /// Empty text reserves one font-size line. Words follow [`wrap_text`]'s
    /// whitespace/character splitting; an overwide glyph occupies its own line
    /// and is clipped to the content width. No shaping or fallback is added.
    ///
    /// A nonfinite/nonpositive width reserves zero space and draws nothing.
    /// A positive width exhausted by margins reserves one empty line. Height
    /// contains every measured glyph even with tight baseline spacing. In this
    /// mode [`Self::size`] supplies only a minimum height; its X is ignored.
    /// Recompute the width after a resize. Font measurement and drawing use the
    /// same skin font, size and current DPI, with existing group clipping.
    ///
    /// ```no_run
    /// use macroquad::ui::{root_ui, widgets::Label};
    /// let mut ui = root_ui();
    /// let width = ui.available_width();
    /// Label::new("A compact menu description that can grow vertically.")
    ///     .wrap(width).multiline(20.).ui(&mut ui);
    /// ```
    pub fn wrap(self, width: f32) -> Self {
        Self {
            wrap: Some(width),
            ..self
        }
    }

    /// Measure the allocation without advancing layout or drawing. Uses the
    /// active label skin and needs a running macroquad context. Wrapped height
    /// can be passed to a caller-owned [`crate::ui::LayoutCursor`].
    /// Measurement may populate the font's glyph cache. Keep the skin and width
    /// unchanged between measuring this builder and drawing it.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// use macroquad::ui::{root_ui, widgets::Label, Layout, LayoutCursor, LayoutSettings};
    ///
    /// let mut ui = root_ui();
    /// // In the root UI, rectangle positions are logical window coordinates.
    /// let mut column = LayoutCursor::new(Rect::new(16., 16., 240., 300.),
    ///     LayoutSettings::new(Layout::Vertical).gap(vec2(0., 8.)));
    /// let label = Label::new("A description that reserves its complete wrapped height.")
    ///     .wrap(column.available_size().x);
    /// let allocation = column.allocate(label.measure(&mut ui));
    /// label.position(allocation.point()).ui(&mut ui);
    /// // The next custom control starts below the measured label and gap.
    /// let button = column.allocate_width(32.);
    /// draw_rectangle_lines(button.x, button.y, button.w, button.h, 1., WHITE);
    /// ```
    pub fn measure(&self, ui: &mut Ui) -> Vec2 {
        let context = ui.get_active_window_context();
        if let Some(width) = self.wrap {
            self.wrapped_layout(&context.style.label_style, width).size
        } else {
            self.size.unwrap_or_else(|| {
                context.window.painter.content_with_margins_size(
                    &context.style.label_style,
                    &UiContent::Label(self.label.clone()),
                )
            })
        }
    }

    fn wrapped_layout(&self, style: &crate::ui::Style, width: f32) -> WrappedLabel {
        let font = style.font.lock().unwrap();
        let dpi = crate::window::screen_dpi_scale();
        let raster_size = (style.font_size as f32 * dpi).ceil() as u16;
        wrapped_layout(
            &self.label,
            width,
            style.border_margin(),
            style.font_size as f32,
            font.ascent(raster_size as f32) / dpi,
            self._multiline,
            self.size.map_or(0., |size| size.y),
            |text, width| wrap_text(text, Some(&font), style.font_size, 1., width * dpi),
            |text| font.measure_text(text, style.font_size, 1., 1., |_| {}),
        )
    }

    pub fn position<P: Into<Option<Vec2>>>(self, position: P) -> Self {
        let position = position.into();

        Label { position, ..self }
    }

    pub fn size(self, size: Vec2) -> Self {
        Label {
            size: Some(size),
            ..self
        }
    }

    pub fn ui(self, ui: &mut Ui) {
        let context = ui.get_active_window_context();

        if let Some(width) = self.wrap {
            let layout = self.wrapped_layout(&context.style.label_style, width);
            if layout.lines.is_empty() {
                return;
            }
            let pos = context.window.cursor.fit(
                layout.size,
                self.position.map_or(Layout::Vertical, Layout::Free),
            );
            if layout.content_width <= 0. {
                return;
            }
            let painter = &mut context.window.painter;
            let previous_clip = painter.clipping_zone;
            let content = Rect::new(
                finite_sum(pos.x, layout.left),
                finite_sum(pos.y, layout.top),
                layout.content_width,
                layout.content_height,
            );
            let clip = match previous_clip {
                Some(previous) => previous
                    .intersect(content)
                    .unwrap_or(Rect::new(0., 0., 0., 0.)),
                None => content,
            };
            painter.set_clip_rect(Some(clip));
            let style = &context.style.label_style;
            let mut font = style.font.lock().unwrap();
            let dpi = crate::window::screen_dpi_scale();
            let raster_size = (style.font_size as f32 * dpi).ceil() as u16;
            let color = style.text_color(ElementState {
                focused: context.focused,
                ..Default::default()
            });
            for (index, line) in layout.lines.iter().enumerate() {
                let offset = (index as f64 * layout.spacing as f64).min(f32::MAX as f64) as f32;
                let mut position = vec2(
                    finite_sum(pos.x, layout.left),
                    finite_sum(
                        pos.y,
                        finite_sum(layout.top, finite_sum(layout.baseline, offset)),
                    ),
                );
                for character in line.chars() {
                    if let Some(advance) = painter.draw_character_scaled(
                        character,
                        position,
                        color,
                        &mut font,
                        raster_size,
                        1. / dpi,
                    ) {
                        position.x = finite_sum(position.x, advance);
                    }
                }
            }
            painter.set_clip_rect(previous_clip);
            return;
        }

        let size = self.size.unwrap_or_else(|| {
            context.window.painter.content_with_margins_size(
                &context.style.label_style,
                &UiContent::Label(self.label.clone()),
            )
        });

        let pos = context
            .window
            .cursor
            .fit(size, self.position.map_or(Layout::Vertical, Layout::Free));

        context.window.painter.draw_element_content(
            &context.style.label_style,
            pos,
            size,
            &UiContent::Label(self.label),
            ElementState {
                focused: context.focused,
                hovered: false,
                clicked: false,
                selected: false,
            },
        );
    }
}

impl Ui {
    pub fn label<P: Into<Option<Vec2>>>(&mut self, position: P, label: &str) {
        Label::new(label).position(position).ui(self);
    }

    pub fn calc_size(&mut self, label: &str) -> Vec2 {
        let context = self.get_active_window_context();

        context
            .window
            .painter
            .content_with_margins_size(&context.style.label_style, &UiContent::Label(label.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{LayoutCursor, LayoutSettings};

    fn layout(
        text: &str,
        width: f32,
        margin: RectOffset,
        spacing: Option<f32>,
        min: f32,
    ) -> WrappedLabel {
        wrapped_layout(
            text,
            width,
            margin,
            16.,
            12.,
            spacing,
            min,
            |text, width| {
                let count = (width / 8.).floor().max(1.) as usize;
                text.chars()
                    .collect::<Vec<_>>()
                    .chunks(count)
                    .map(|chars| chars.iter().collect::<String>())
                    .collect::<Vec<_>>()
                    .join("\n")
            },
            |text| TextDimensions {
                width: text.chars().count() as f32 * 8.,
                height: 15.,
                offset_y: 12.,
            },
        )
    }

    #[test]
    fn wrapped_empty_hard_breaks_and_asymmetric_margins_reserve_lines() {
        let margin = RectOffset::new(3., 7., 5., 11.);
        let empty = layout("", 90., margin, None, 0.);
        assert_eq!(empty.lines, [""]);
        assert_eq!(empty.size, vec2(90., 32.));
        let hard = layout("a\r\n\nb\n", 90., margin, None, 0.);
        assert_eq!(hard.lines, ["a", "", "b", ""]);
        assert_eq!(hard.size, vec2(90., 80.));
        assert_eq!((hard.left, hard.top, hard.content_width), (3., 5., 80.));
    }

    #[test]
    fn wrapped_resizing_changes_column_height_and_scroll_extent() {
        let text = "abcdefghijabcdefghij";
        let compact = layout(text, 40., RectOffset::default(), None, 0.);
        let desktop = layout(text, 160., RectOffset::default(), None, 0.);
        assert_eq!(compact.lines.len(), 4);
        assert_eq!(desktop.lines.len(), 1);
        for wrapped in [compact, desktop] {
            let mut column = LayoutCursor::new(
                Rect::new(0., 0., wrapped.size.x, 30.),
                LayoutSettings::new(Layout::Vertical).gap(vec2(0., 5.)),
            );
            column.allocate(wrapped.size);
            let next = column.allocate(vec2(20., 10.));
            assert_eq!(next.y, wrapped.size.y + 5.);
            assert_eq!(column.content_size().y, wrapped.size.y + 15.);
            let mut row = LayoutCursor::new(
                Rect::new(0., 0., 300., 30.),
                LayoutSettings::new(Layout::Horizontal).gap(vec2(7., 0.)),
            );
            row.allocate(wrapped.size);
            assert_eq!(row.allocate(vec2(20., 10.)).x, wrapped.size.x + 7.);
            assert_eq!(row.content_size().y, wrapped.size.y);
        }
    }

    #[test]
    fn wrapped_tight_spacing_contains_tall_ink_and_minimum_height() {
        let result = wrapped_layout(
            "a\nb",
            100.,
            RectOffset::default(),
            16.,
            10.,
            Some(2.),
            0.,
            |text, _| text.to_owned(),
            |_| TextDimensions {
                width: 8.,
                height: 25.,
                offset_y: 19.,
            },
        );
        assert_eq!(result.baseline, 19.);
        assert_eq!(result.size, vec2(100., 27.));
        assert_eq!(
            layout("a\nb", 100., RectOffset::default(), Some(2.), 80.)
                .size
                .y,
            80.
        );
        for spacing in [0., -1., f32::NAN, f32::INFINITY] {
            assert_eq!(
                layout("a\nb", 100., RectOffset::default(), Some(spacing), 0.)
                    .size
                    .y,
                32.
            );
        }
    }

    #[test]
    fn wrapped_invalid_and_exhausted_widths_never_measure_or_wrap() {
        for width in [0., -1., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let result = wrapped_layout(
                "text",
                width,
                RectOffset::default(),
                16.,
                12.,
                None,
                99.,
                |_, _| panic!("invalid width must not wrap"),
                |_| panic!("invalid width must not measure"),
            );
            assert_eq!(result.size, Vec2::ZERO);
            assert!(result.lines.is_empty());
        }
        let exhausted = layout("text", 5., RectOffset::new(3., 4., 2., 8.), None, 0.);
        assert_eq!(exhausted.size, vec2(5., 26.));
        assert_eq!(exhausted.lines, [""]);
        let narrow = layout("é漢a", 0.1, RectOffset::default(), None, 0.);
        assert_eq!(narrow.lines, ["é", "漢", "a"]);
        assert_eq!(narrow.size, vec2(0.1, 48.));
    }

    #[test]
    fn wrapped_nonfinite_margins_and_huge_spacing_stay_finite() {
        let result = layout(
            "a\nb\nc",
            20.,
            RectOffset::new(-1., f32::NAN, f32::INFINITY, -9.),
            Some(f32::MAX),
            0.,
        );
        assert_eq!(result.left, 0.);
        assert_eq!(result.top, 0.);
        assert_eq!(result.size, vec2(20., f32::MAX));
        let synthetic = wrapped_layout(
            "wide",
            1.,
            RectOffset::default(),
            16.,
            12.,
            None,
            0.,
            |_, _| "\n\nw\n\ni\n\nd\n\ne".into(),
            |_| TextDimensions {
                width: 8.,
                height: 12.,
                offset_y: 12.,
            },
        );
        assert_eq!(synthetic.lines, ["w", "i", "d", "e"]);
    }
}
