//! Public geometry regressions; no window, font or GPU is initialized.
use macroquad::{
    math::{vec2, Rect, RectOffset},
    ui::{Layout, LayoutAlign, LayoutCursor, LayoutSettings},
};

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
}

#[test]
fn column_separates_padding_gaps_and_cross_axis_alignment() {
    let settings = LayoutSettings::new(Layout::Vertical)
        .padding(RectOffset::new(10., 20., 30., 40.))
        .gap(vec2(9., 7.))
        .align(LayoutAlign::End);
    let mut cursor = LayoutCursor::new(Rect::new(100., 200., 300., 400.), settings);
    assert_eq!(cursor.available_size(), vec2(270., 330.));
    assert_eq!(cursor.content_size(), vec2(30., 70.));
    assert_eq!(
        cursor.allocate(vec2(50., 20.)),
        Rect::new(330., 230., 50., 20.)
    );
    assert_eq!(cursor.available_size(), vec2(270., 303.));
    assert_eq!(cursor.allocate_width(32.), Rect::new(110., 257., 270., 32.));
    assert_eq!(cursor.content_size(), vec2(300., 129.));
}

#[test]
fn horizontal_stays_one_scrollable_row_and_allocates_remaining_width() {
    let settings = LayoutSettings::new(Layout::Horizontal)
        .padding(RectOffset::new(5., 15., 20., 10.))
        .gap(vec2(8., 99.))
        .align(LayoutAlign::Center);
    let mut cursor = LayoutCursor::new(Rect::new(10., 30., 180., 100.), settings);
    assert_eq!(
        cursor.allocate(vec2(50., 30.)),
        Rect::new(15., 70., 50., 30.)
    );
    assert_eq!(cursor.available_size(), vec2(102., 70.));
    assert_eq!(cursor.allocate_width(50.), Rect::new(73., 60., 102., 50.));
    // Exhausting the viewport does not introduce a hidden, implicit row wrap.
    assert_eq!(
        cursor.allocate(vec2(40., 90.)),
        Rect::new(183., 50., 40., 90.)
    );
    assert_eq!(cursor.available_size(), vec2(0., 70.));
    assert_eq!(cursor.content_size(), vec2(228., 120.));
}

#[test]
fn oversized_children_keep_their_leading_edge_reachable() {
    for align in [LayoutAlign::Start, LayoutAlign::Center, LayoutAlign::End] {
        let mut column = LayoutCursor::new(
            Rect::new(50., 70., 100., 80.),
            LayoutSettings::new(Layout::Vertical)
                .padding(RectOffset::new(10., 20., 5., 15.))
                .align(align),
        );
        assert_eq!(
            column.allocate(vec2(150., 20.)),
            Rect::new(60., 75., 150., 20.)
        );
        assert_eq!(column.content_size(), vec2(180., 40.));
        let mut row = LayoutCursor::new(
            Rect::new(50., 70., 100., 80.),
            LayoutSettings::new(Layout::Horizontal).align(align),
        );
        assert_eq!(
            row.allocate(vec2(20., 130.)),
            Rect::new(50., 70., 20., 130.)
        );
    }
}

#[test]
fn free_offsets_include_padding_without_advancing_sequential_position() {
    let mut cursor = LayoutCursor::new(
        Rect::new(50., 100., 200., 180.),
        LayoutSettings::new(Layout::Free(vec2(8., 12.)))
            .padding(RectOffset::new(10., 20., 30., 40.))
            .gap(vec2(99., 99.))
            .align(LayoutAlign::End),
    );
    assert_eq!(
        cursor.allocate(vec2(20., 30.)),
        Rect::new(68., 142., 20., 30.)
    );
    assert_eq!(
        cursor.allocate(vec2(50., 10.)),
        Rect::new(68., 142., 50., 10.)
    );
    assert_eq!(cursor.available_size(), vec2(170., 110.));
    assert_eq!(
        cursor.allocate_at(vec2(180., 160.), vec2(25., 35.)),
        Rect::new(240., 290., 25., 35.)
    );
    assert_eq!(cursor.content_size(), vec2(235., 265.));
}

#[test]
fn empty_and_invalid_geometry_remains_finite_and_nonnegative() {
    let mut cursor = LayoutCursor::new(
        Rect::new(f32::NAN, f32::NEG_INFINITY, -10., f32::INFINITY),
        LayoutSettings::new(Layout::Vertical)
            .padding(RectOffset::new(-1., f32::NAN, f32::INFINITY, -5.))
            .gap(vec2(f32::NAN, -2.)),
    );
    assert_eq!(cursor.available_size(), vec2(0., 0.));
    assert_eq!(
        cursor.allocate(vec2(-2., f32::INFINITY)),
        Rect::new(0., 0., 0., 0.)
    );
    assert_eq!(cursor.allocate_width(f32::NAN), Rect::new(0., 0., 0., 0.));
    assert_eq!(cursor.content_size(), vec2(0., 0.));
}

#[test]
fn viewport_translation_does_not_change_local_allocation_or_content_extents() {
    for width in [0., 1., 90., 390., 1280.] {
        for align in [LayoutAlign::Start, LayoutAlign::Center, LayoutAlign::End] {
            for layout in [
                Layout::Vertical,
                Layout::Horizontal,
                Layout::Free(vec2(6., 9.)),
            ] {
                let settings = LayoutSettings::new(layout)
                    .padding(RectOffset::new(7., 11., 13., 17.))
                    .gap(vec2(3., 5.))
                    .align(align);
                let mut origin =
                    LayoutCursor::new(Rect::new(0., 0., width, 400.), settings.clone());
                let mut offset = LayoutCursor::new(Rect::new(137., -93., width, 400.), settings);
                for size in [vec2(35., 18.), vec2(500., 70.), vec2(21., 800.)] {
                    let a = origin.allocate(size);
                    let b = offset.allocate(size);
                    close(b.x - a.x, 137.);
                    close(b.y - a.y, -93.);
                    assert_eq!(origin.available_size(), offset.available_size());
                    assert_eq!(origin.content_size(), offset.content_size());
                }
            }
        }
    }
}
