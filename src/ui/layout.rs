//! Small, immediate layout helpers shared by widgets and custom drawing.

use crate::math::{vec2, Rect, RectOffset, Vec2};

use super::Layout;

/// Cross-axis placement in a row or column. Oversized children stay at the start
/// so their leading edge remains reachable by the existing scrolling UI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayoutAlign {
    #[default]
    Start,
    Center,
    End,
}

/// Opt-in spacing and flow for an immediate UI container.
///
/// Vertical columns and horizontal rows use only their corresponding gap
/// component. Rows do not wrap: choose a column at a responsive breakpoint.
/// Built-in groups scroll oversized columns vertically; horizontal overflow
/// requires caller handling. `Free` positions are relative to padding.
/// These values replace the skin's cursor margin, not widget skin margins.
///
/// [`LayoutAlign`] affects the cross axis: horizontal placement in a column,
/// vertical placement in a row. Select the flow from the available width;
/// the settings do not choose responsive breakpoints for the application.
///
/// ```
/// use macroquad::prelude::*;
/// use macroquad::ui::{Layout, LayoutAlign, LayoutCursor, LayoutSettings};
/// let settings = LayoutSettings::new(Layout::Vertical)
///     .padding(RectOffset::new(12., 12., 8., 8.))
///     .gap(vec2(0., 6.))
///     .align(LayoutAlign::Center);
/// let mut column = LayoutCursor::new(Rect::new(0., 0., 240., 100.), settings);
/// assert_eq!(column.allocate(vec2(100., 24.)), Rect::new(70., 8., 100., 24.));
/// assert_eq!(column.allocate_width(24.), Rect::new(12., 38., 216., 24.));
/// assert_eq!(column.content_size(), vec2(240., 70.));
/// ```
#[derive(Clone, Debug)]
pub struct LayoutSettings {
    layout: Layout,
    padding: RectOffset,
    gap: Vec2,
    align: LayoutAlign,
}

impl LayoutSettings {
    pub const fn new(layout: Layout) -> Self {
        Self {
            layout,
            padding: RectOffset::new(0., 0., 0., 0.),
            gap: Vec2::ZERO,
            align: LayoutAlign::Start,
        }
    }

    pub const fn padding(self, padding: RectOffset) -> Self {
        Self { padding, ..self }
    }

    pub const fn gap(self, gap: Vec2) -> Self {
        Self { gap, ..self }
    }

    pub const fn align(self, align: LayoutAlign) -> Self {
        Self { align, ..self }
    }
}

impl Default for LayoutSettings {
    fn default() -> Self {
        Self::new(Layout::Vertical)
    }
}

/// Pure rectangle allocation with the same settings as [`super::Ui::set_layout`].
///
/// This helper does not draw or access a GPU. Coordinates use the caller's
/// space; for `root_ui` and a default-camera HUD use logical window units.
/// Recreate it when the container resizes. Padding/gaps and requested sizes
/// that are negative or nonfinite become zero. Oversized children retain their
/// requested size and contribute to [`Self::content_size`], allowing scrolling.
/// Finite arithmetic saturates at `f32::MAX` rather than producing infinity.
///
/// ```
/// use macroquad::prelude::*;
/// use macroquad::ui::{Layout, LayoutCursor, LayoutSettings};
/// let settings = LayoutSettings::new(Layout::Horizontal).gap(vec2(8., 0.));
/// let mut row = LayoutCursor::new(Rect::new(10., 20., 300., 40.), settings);
/// let left = row.allocate(vec2(100., 40.));
/// let right = row.allocate_width(40.);
/// assert_eq!(left.x, 10.);
/// assert_eq!(right, Rect::new(118., 20., 192., 40.));
/// ```
#[derive(Clone, Debug)]
pub struct LayoutCursor {
    area: Rect,
    settings: LayoutSettings,
    next: Vec2,
    extent: Vec2,
}

fn nonnegative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.)
    } else {
        0.
    }
}

fn finite(value: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        0.
    }
}

fn add(a: f32, b: f32) -> f32 {
    (a as f64 + b as f64).clamp(-(f32::MAX as f64), f32::MAX as f64) as f32
}

impl LayoutCursor {
    /// Start an empty layout in `area`, whose origin and size use the caller's
    /// coordinate space. No clipping, scrolling, drawing or hit testing is added.
    pub fn new(area: Rect, mut settings: LayoutSettings) -> Self {
        settings.padding = RectOffset::new(
            nonnegative(settings.padding.left),
            nonnegative(settings.padding.right),
            nonnegative(settings.padding.top),
            nonnegative(settings.padding.bottom),
        );
        settings.gap = vec2(nonnegative(settings.gap.x), nonnegative(settings.gap.y));
        let start = vec2(settings.padding.left, settings.padding.top);
        Self {
            area: Rect::new(
                finite(area.x),
                finite(area.y),
                nonnegative(area.w),
                nonnegative(area.h),
            ),
            settings,
            next: start,
            extent: start,
        }
    }

    fn inner_size(&self) -> Vec2 {
        vec2(
            (self.area.w - self.settings.padding.left - self.settings.padding.right).max(0.),
            (self.area.h - self.settings.padding.top - self.settings.padding.bottom).max(0.),
        )
    }

    /// Remaining main-axis space and the full padded cross-axis size.
    /// Scrolling does not affect this geometry. Exhausted dimensions are zero.
    pub fn available_size(&self) -> Vec2 {
        let inner = self.inner_size();
        match self.settings.layout {
            Layout::Horizontal => vec2(
                (self.area.w - self.settings.padding.right - self.next.x).max(0.),
                inner.y,
            ),
            Layout::Vertical => vec2(
                inner.x,
                (self.area.h - self.settings.padding.bottom - self.next.y).max(0.),
            ),
            Layout::Free(_) => inner,
        }
    }

    /// The X component of [`Self::available_size`]; rows consume it as children
    /// are allocated, while columns retain their padded inner width.
    pub fn available_width(&self) -> f32 {
        self.available_size().x
    }

    fn aligned(&self, available: f32, size: f32) -> f32 {
        let spare = (available - size).max(0.);
        match self.settings.align {
            LayoutAlign::Start => 0.,
            LayoutAlign::Center => spare * 0.5,
            LayoutAlign::End => spare,
        }
    }

    /// Allocate one child, advancing a row/column by its size and one gap.
    /// Free placement does not advance the flow cursor.
    pub fn allocate(&mut self, size: Vec2) -> Rect {
        let size = vec2(nonnegative(size.x), nonnegative(size.y));
        let inner = self.inner_size();
        let offset = match self.settings.layout {
            Layout::Horizontal => vec2(
                self.next.x - self.settings.padding.left,
                self.aligned(inner.y, size.y),
            ),
            Layout::Vertical => vec2(
                self.aligned(inner.x, size.x),
                self.next.y - self.settings.padding.top,
            ),
            Layout::Free(offset) => offset,
        };
        let rect = self.allocate_at(offset, size);
        match self.settings.layout {
            Layout::Horizontal => self.next.x = add(add(self.next.x, size.x), self.settings.gap.x),
            Layout::Vertical => self.next.y = add(add(self.next.y, size.y), self.settings.gap.y),
            Layout::Free(_) => {}
        }
        rect
    }

    /// Allocate the currently available width with an explicit height.
    pub fn allocate_width(&mut self, height: f32) -> Rect {
        self.allocate(vec2(self.available_width(), height))
    }

    /// Reserve a child at an offset from the padded origin without advancing
    /// the row/column. Negative finite offsets are allowed and may be clipped.
    pub fn allocate_at(&mut self, offset: Vec2, size: Vec2) -> Rect {
        let x = add(self.settings.padding.left, finite(offset.x));
        let y = add(self.settings.padding.top, finite(offset.y));
        let w = nonnegative(size.x);
        let h = nonnegative(size.y);
        self.extent = self.extent.max(vec2(add(x, w), add(y, h)));
        Rect::new(add(self.area.x, x), add(self.area.y, y), w, h)
    }

    /// Required content size including trailing padding, excluding the gap
    /// after the last child. Empty content consists only of padding.
    pub fn content_size(&self) -> Vec2 {
        vec2(
            add(self.extent.x, self.settings.padding.right),
            add(self.extent.y, self.settings.padding.bottom),
        )
    }

    pub(crate) fn position(&self) -> Vec2 {
        self.next
    }

    pub(crate) fn allocate_widget(&mut self, size: Vec2, free: Option<Vec2>, indent: f32) -> Rect {
        let width = self.area.w;
        self.area.w = (width - indent).max(0.);
        let rect = match free {
            Some(offset) => self.allocate_at(offset, size),
            None => self.allocate(size),
        };
        self.area.w = width;
        self.extent.x = self
            .extent
            .x
            .max(add(add(rect.x - self.area.x, rect.w), indent));
        rect
    }
}
