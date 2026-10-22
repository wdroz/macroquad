//! Exercise the recipe's real allocation path without a font, window or GPU.
#[path = "../examples/support/custom_wrapped_hud_layout.rs"]
mod layout;
use layout::{arrange, panel_limit, text_width, FONT_SCALE, FONT_SIZE, LINE_DISTANCE};
use macroquad::{math::vec2, text::TextDimensions};

fn metrics(lines: usize, offset_y: f32) -> TextDimensions {
    TextDimensions {
        width: 200.,
        height: lines as f32 * FONT_SIZE as f32 * FONT_SCALE * LINE_DISTANCE,
        offset_y,
    }
}

#[test]
fn short_text_grows_the_panel_from_measured_height_and_uses_its_baseline() {
    let limit = panel_limit(vec2(960., 640.));
    let panel = arrange(limit, metrics(4, 13.), 0.);
    assert_eq!(panel.requested_height, 129.);
    assert_eq!(panel.panel.h, 129.);
    assert_eq!(panel.clip.h, 105.);
    assert_eq!(panel.baseline, vec2(28., 89.));
    assert_eq!(panel.baseline.y - 13., panel.clip.y);
    assert_eq!(panel.scroll_max, 0.);
    assert!(panel.panel.bottom() < limit.bottom());
}

#[test]
fn bounded_overflow_scrolls_to_the_final_reserved_line_without_moving_the_panel() {
    let limit = panel_limit(vec2(390., 400.));
    let top = arrange(limit, metrics(12, 17.), 0.);
    let bottom = arrange(limit, metrics(12, 17.), f32::MAX);
    assert_eq!(top.panel, bottom.panel);
    assert_eq!(top.panel.h, 284.);
    assert_eq!(top.clip.h, 260.);
    assert_eq!(bottom.scroll, 55.);
    assert_eq!(bottom.scroll_max, 55.);
    // Bottom of the measured allocation lands at the inner clip's bottom.
    assert_eq!(bottom.baseline.y - 17. + 315., bottom.clip.bottom());
    assert_eq!(top.baseline.y - bottom.baseline.y, bottom.scroll);
}

#[test]
fn responsive_rewrap_has_a_new_width_and_discards_obsolete_scroll() {
    let compact = panel_limit(vec2(390., 400.));
    let desktop = panel_limit(vec2(1280., 720.));
    assert_eq!(text_width(compact), 334.);
    assert_eq!(text_width(desktop), 576.);
    let desktop_notes = arrange(desktop, metrics(9, 17.), 0.);
    assert_eq!(desktop_notes.panel.h, 260.25);
    assert_eq!(desktop_notes.panel.h, desktop_notes.requested_height);
    assert_eq!(desktop_notes.scroll_max, 0.);
    let old = arrange(compact, metrics(12, 17.), f32::MAX);
    // The caller rewraps/re-measures at the new width before arranging again.
    let resized = arrange(desktop, metrics(5, 17.), old.scroll);
    assert_eq!(resized.scroll, 0.);
    assert_eq!(resized.panel.h, 155.25);
    let back = arrange(compact, metrics(12, 17.), resized.scroll);
    assert_eq!(back.scroll, 0.);
    assert_eq!(back.panel, old.panel);
}

#[test]
fn blank_lines_reserve_full_advance_and_short_windows_keep_the_footer_clear() {
    let limit = panel_limit(vec2(390., 180.));
    let one = arrange(limit, metrics(1, 11.), 0.);
    let blank_line = arrange(limit, metrics(2, 11.), 0.);
    assert_eq!(blank_line.requested_height - one.requested_height, 26.25);
    assert_eq!(blank_line.baseline, one.baseline);
    assert_eq!(blank_line.panel.bottom(), 128.);
    assert_eq!(180. - blank_line.panel.bottom(), 52.);
    assert_eq!(blank_line.scroll_max, 12.5);
}

#[test]
fn empty_or_collapsed_content_does_not_invent_a_scroll_or_negative_clip() {
    for size in [vec2(0., 0.), vec2(32., 116.), vec2(40., 125.)] {
        let limit = panel_limit(size);
        let empty = arrange(limit, TextDimensions::default(), f32::MAX);
        assert_eq!(text_width(limit), 0.);
        assert_eq!(empty.scroll, 0.);
        assert_eq!(empty.scroll_max, 0.);
        assert!(empty.clip.w >= 0. && empty.clip.h >= 0.);
        assert!(empty.clip.x >= empty.panel.x && empty.clip.right() <= empty.panel.right());
        assert!(empty.clip.y >= empty.panel.y && empty.clip.bottom() <= empty.panel.bottom());
        let text = arrange(limit, metrics(3, 11.), f32::NAN);
        assert_eq!(text.scroll, 0.);
        assert!(text.baseline.is_finite());
    }
}

#[test]
fn narrow_positive_width_uses_the_recipe_readability_breakpoint() {
    // At 57px the padded width is 1px: passing it to wrap_text can introduce
    // an empty first line before a single overwide glyph. Hide the notes here.
    for width in [57., 80., 97.] {
        let limit = panel_limit(vec2(width, 400.));
        assert_eq!(text_width(limit), 0.);
        let hidden = arrange(limit, TextDimensions::default(), 100.);
        assert_eq!(hidden.requested_height, 24.);
        assert_eq!(hidden.scroll, 0.);
    }
    assert_eq!(text_width(panel_limit(vec2(98., 400.))), 42.);
}
