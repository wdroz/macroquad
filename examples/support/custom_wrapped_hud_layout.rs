//! Example-owned geometry in logical window pixels; no font, window or GPU access.
use macroquad::{
    math::{vec2, Rect, RectOffset, Vec2},
    text::TextDimensions,
    ui::{Layout, LayoutCursor, LayoutSettings},
};

pub const FONT_SIZE: u16 = 24;
pub const FONT_SCALE: f32 = 0.875;
pub const LINE_DISTANCE: f32 = 1.25;
const PADDING: f32 = 12.;

/// The application chooses these responsive limits, independently of text size.
pub fn panel_limit(size: Vec2) -> Rect {
    Rect::new(
        16.,
        64.,
        (size.x - 32.).clamp(0., 600.),
        (size.y - 116.).clamp(0., 300.),
    )
}

fn column(area: Rect) -> LayoutCursor {
    LayoutCursor::new(
        area,
        LayoutSettings::new(Layout::Vertical)
            .padding(RectOffset::new(PADDING, PADDING, PADDING, PADDING)),
    )
}

pub fn text_width(limit: Rect) -> f32 {
    let width = column(limit).available_width();
    // This recipe hides its notes below two scaled ems, rather than wrapping a
    // glyph into an unreadable sliver (which may introduce a leading blank).
    if width >= 2. * FONT_SIZE as f32 * FONT_SCALE {
        width
    } else {
        0.
    }
}

#[derive(Debug)]
pub struct PanelLayout {
    pub panel: Rect,
    pub clip: Rect,
    pub baseline: Vec2,
    pub scroll: f32,
    pub scroll_max: f32,
    pub requested_height: f32,
}

/// `measured` comes from the same wrapped string/font/size/scale used to draw.
pub fn arrange(limit: Rect, measured: TextDimensions, scroll: f32) -> PanelLayout {
    let mut cursor = column(limit);
    let text = cursor.allocate_width(measured.height);
    let requested_height = cursor.content_size().y;
    let panel = Rect::new(limit.x, limit.y, limit.w, requested_height.min(limit.h));
    // Collapsed windows still get an empty, nonnegative clip inside the panel.
    let inset = vec2(PADDING.min(panel.w * 0.5), PADDING.min(panel.h * 0.5));
    let clip = Rect::new(
        panel.x + inset.x,
        panel.y + inset.y,
        (panel.w - 2. * inset.x).max(0.),
        (panel.h - 2. * inset.y).max(0.),
    );
    let scroll_max = (measured.height - clip.h).max(0.);
    let scroll = if scroll.is_finite() {
        scroll.clamp(0., scroll_max)
    } else {
        0.
    };
    PanelLayout {
        panel,
        clip,
        // Draw APIs take a baseline, while LayoutCursor reserves a top edge.
        baseline: vec2(text.x, text.y + measured.offset_y - scroll),
        scroll,
        scroll_max,
        requested_height,
    }
}
