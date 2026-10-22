#[path = "../examples/support/document_zoom_pan_state.rs"]
mod state;
use macroquad::prelude::*;
use state::{contains, viewport, Document, Input, MARKER, PAGE};

fn close(a: Vec2, b: Vec2) {
    assert!((a - b).length() < 0.002, "{a:?} != {b:?}");
}

fn wheel(document: &mut Document, pointer: Vec2, delta: Vec2, zoom: bool) {
    assert!(document.update(Input {
        pointer,
        wheel: delta,
        zoom_modifier: zoom,
        ..Default::default()
    }));
    assert!(document.bounded());
}

fn press(document: &mut Document, pointer: Vec2) {
    document.update(Input {
        pointer,
        pressed: true,
        down: true,
        ..Default::default()
    });
}

fn release(document: &mut Document, pointer: Vec2) {
    document.update(Input {
        pointer,
        released: true,
        ..Default::default()
    });
}

#[test]
fn compact_and_desktop_fit_and_inverse_mapping() {
    for size in [vec2(390., 400.), vec2(960., 640.), vec2(1600., 1200.)] {
        let document = Document::new(viewport(size));
        assert!(document.bounded());
        assert!(document.follow_fit);
        close(
            document.page_to_window(PAGE * 0.5),
            document.origin() + document.size() * 0.5,
        );
        for page in [Vec2::ZERO, PAGE, PAGE * 0.5, vec2(-18., 1100.)] {
            close(document.window_to_page(document.page_to_window(page)), page);
        }
        // Exercise the actual public camera matrix, including its window
        // Y orientation, independently of the example's affine inverse.
        let camera = Camera2D::from_display_rect(document.camera_rect(size));
        for dpi in [1., 2.] {
            for page in [Vec2::ZERO, PAGE, PAGE * 0.5, vec2(MARKER.x, MARKER.y)] {
                let pixel = camera.world_to_target_with_size(page, size * dpi).unwrap();
                close(pixel, document.page_to_window(page) * dpi);
                let back = camera.target_to_world_with_size(pixel, size * dpi).unwrap();
                close(back, page);
            }
        }
        assert!(state::FIT_BUTTON.y + state::FIT_BUTTON.h < document.viewport.y);
        assert!(state::RESET_BUTTON.x + state::RESET_BUTTON.w < size.x);
        assert!(document.viewport.y + document.viewport.h < size.y - 24.);
    }
}

#[test]
fn zoom_preserves_pointer_anchor_when_bounds_allow_it() {
    let mut document = Document::new(viewport(vec2(390., 400.)));
    document.reset();
    let pointer = document.origin() + document.size() * vec2(0.63, 0.41);
    let page = document.window_to_page(pointer);
    wheel(&mut document, pointer, vec2(0., 3.), true);
    close(document.page_to_window(page), pointer);
    assert!(document.anchor_error < 0.002);
    assert!(!document.anchor_clamped);
    assert!(!document.follow_fit);
    wheel(&mut document, pointer, vec2(0., -2.), true);
    close(document.page_to_window(page), pointer);
    assert_eq!(document.zooms, 2);
}

#[test]
fn zoom_reports_required_bounds_adjustment_and_limits() {
    let mut document = Document::new(viewport(vec2(960., 640.)));
    // At fit, the horizontal axis is undersized and must stay centered;
    // off-center pointer anchoring cannot override this constraint.
    let pointer = document.origin() + vec2(8., 8.);
    wheel(&mut document, pointer, vec2(0., 1.), true);
    assert!(document.anchor_clamped);
    assert!(document.anchor_error > 1.);
    for y in [100000., -100000., 100000., -100000.] {
        wheel(&mut document, pointer, vec2(0., y), true);
        assert!(document.zoom >= document.fit_zoom().min(0.2));
        assert!(document.zoom <= state::MAX_ZOOM);
    }
}

#[test]
fn wheel_and_hit_testing_are_gated_by_half_open_viewport() {
    let mut document = Document::new(viewport(vec2(390., 400.)));
    document.reset();
    let before = (document.zoom, document.pan, document.follow_fit);
    for pointer in [
        vec2(20., 30.),
        vec2(0., 200.),
        document.origin() + vec2(document.viewport.w, 20.),
        document.origin() + vec2(20., document.viewport.h),
    ] {
        wheel(&mut document, pointer, vec2(1., 3.), false);
        wheel(&mut document, pointer, vec2(0., 3.), true);
        press(&mut document, pointer);
        release(&mut document, pointer);
        assert_eq!((document.zoom, document.pan, document.follow_fit), before);
        assert!(!document.captured());
    }
    assert_eq!(document.wheels, 0);
    assert_eq!(document.rejected_wheels, 8);
    assert_eq!(document.hits, 0);
    assert_eq!(document.outside_presses, 4);
    // A visible generated-page hit is distinct from merely being in the page.
    let marker =
        document.page_to_window(vec2(MARKER.x + MARKER.w * 0.5, MARKER.y + MARKER.h * 0.5));
    assert!(document.marker_hit(marker));
    // Part of the note itself is hidden above this zoomed viewport.
    let hidden_note = document.page_to_window(vec2(MARKER.x + 10., MARKER.y + 10.));
    assert!(contains(MARKER, document.window_to_page(hidden_note)));
    assert!(!document.marker_hit(hidden_note));
    assert!(!contains(
        document.viewport,
        document.page_to_window(vec2(0., 0.))
    ));
    assert!(!document.marker_hit(document.page_to_window(vec2(0., 0.))));
}

#[test]
fn both_scroll_axes_clamp_and_small_axes_remain_centered() {
    let mut document = Document::new(viewport(vec2(390., 400.)));
    document.reset();
    let pointer = document.origin() + document.size() * 0.5;
    wheel(&mut document, pointer, vec2(-100., -100.), false);
    close(document.pan, document.size() - PAGE);
    wheel(&mut document, pointer, vec2(100., 100.), false);
    close(document.pan, Vec2::ZERO);
    document.fit();
    let fit_pan = document.pan;
    wheel(&mut document, pointer, vec2(-100., 100.), false);
    close(document.pan, fit_pan);
}

#[test]
fn click_release_selects_only_visible_marker_and_small_motion_is_not_drag() {
    let mut document = Document::new(viewport(vec2(390., 400.)));
    let marker =
        document.page_to_window(vec2(MARKER.x + MARKER.w * 0.5, MARKER.y + MARKER.h * 0.5));
    press(&mut document, marker);
    document.update(Input {
        pointer: marker + vec2(1., 0.),
        down: true,
        ..Default::default()
    });
    release(&mut document, marker + vec2(1., 0.));
    assert_eq!(document.hits, 1);
    assert!(document.selected);
    press(&mut document, marker);
    release(&mut document, marker);
    assert_eq!(document.hits, 2);
    assert!(!document.selected);
    // A release without a captured press and a release far from the press
    // must never turn into a click, even if the last rendered position hit.
    release(&mut document, marker);
    press(&mut document, marker);
    release(&mut document, marker + vec2(10., 0.));
    assert_eq!(document.hits, 2);
}

#[test]
fn drag_capture_continues_outside_and_is_bounded_without_clicking() {
    let mut document = Document::new(viewport(vec2(390., 400.)));
    document.reset();
    let marker =
        document.page_to_window(vec2(MARKER.x + MARKER.w * 0.5, MARKER.y + MARKER.h * 0.5));
    press(&mut document, marker);
    for pointer in [vec2(-10000., -10000.), vec2(10000., 10000.)] {
        document.update(Input {
            pointer,
            down: true,
            ..Default::default()
        });
        assert!(document.captured());
        assert!(document.bounded());
    }
    close(document.pan, Vec2::ZERO);
    release(&mut document, vec2(10000., 10000.));
    assert_eq!(document.hits, 0);
    assert_eq!(document.drag_frames, 2);
    assert!(!document.captured());
    let pan = document.pan;
    document.update(Input {
        pointer: marker - vec2(30., 20.),
        down: true,
        ..Default::default()
    });
    close(document.pan, pan);
}

#[test]
fn resize_tracks_fit_or_preserves_manual_center_and_cancels_capture() {
    let mut document = Document::new(viewport(vec2(390., 400.)));
    document.resize(viewport(vec2(960., 640.)));
    assert_eq!(document.zoom, document.fit_zoom());
    assert!(document.follow_fit);
    document.reset();
    let old_center = document.window_to_page(document.origin() + document.size() * 0.5);
    let pointer = document.origin() + document.size() * 0.5;
    press(&mut document, pointer);
    assert!(document.captured());
    assert!(document.resize(viewport(vec2(390., 400.))));
    close(
        document.window_to_page(document.origin() + document.size() * 0.5),
        old_center,
    );
    assert!(!document.captured());
    assert!(!document.follow_fit);
    assert!(!document.resize(document.viewport));
    assert_eq!(document.resizes, 2);
    document.resize(viewport(vec2(4000., 4000.)));
    assert!(document.bounded());
    close(document.pan, (document.size() - PAGE) * 0.5);
}

#[test]
fn fit_and_reset_restore_view_and_preserve_selection_diagnostics() {
    let mut document = Document::new(viewport(vec2(390., 400.)));
    let marker = document.page_to_window(vec2(MARKER.x + 10., MARKER.y + 10.));
    press(&mut document, marker);
    release(&mut document, marker);
    document.reset();
    assert_eq!(document.zoom, 1.);
    assert!(!document.follow_fit);
    assert_eq!(document.hits, 1);
    assert!(document.selected);
    press(&mut document, marker);
    document.fit();
    assert_eq!(document.zoom, document.fit_zoom());
    assert!(document.follow_fit);
    assert!(!document.captured());
    assert!(document.selected);
}

#[test]
fn nonfinite_input_is_ignored_and_lost_button_releases_capture() {
    let mut document = Document::new(viewport(vec2(390., 400.)));
    let pointer = document.origin() + document.size() * 0.5;
    let before = (document.zoom, document.pan);
    for wheel in [vec2(f32::NAN, 1.), vec2(0., f32::INFINITY)] {
        document.update(Input {
            pointer,
            wheel,
            zoom_modifier: true,
            ..Default::default()
        });
    }
    assert_eq!((document.zoom, document.pan), before);
    press(&mut document, pointer);
    document.update(Input {
        pointer: vec2(f32::NAN, 0.),
        down: true,
        ..Default::default()
    });
    assert!(!document.captured());
    press(&mut document, pointer);
    document.update(Input {
        pointer,
        ..Default::default()
    });
    assert!(!document.captured());
    assert!(document.bounded());
}

#[test]
fn repeated_compact_desktop_zoom_scroll_and_resize_stay_finite_and_bounded() {
    let mut document = Document::new(viewport(vec2(390., 400.)));
    for index in 0..600 {
        if index % 11 == 0 {
            let size = [vec2(390., 400.), vec2(960., 640.), vec2(32., 164.)][index % 3];
            document.resize(viewport(size));
        }
        let pointer = document.origin() + document.size() * vec2(0.3, 0.7);
        wheel(
            &mut document,
            pointer,
            vec2((index % 7) as f32 - 3., (index % 9) as f32 - 4. + 0.1),
            index % 2 == 0,
        );
        let round_trip = document.window_to_page(document.page_to_window(PAGE * 0.5));
        assert!((round_trip - PAGE * 0.5).length() * document.zoom < 0.002);
        if index % 19 == 0 {
            document.fit();
        }
        if index % 23 == 0 {
            document.reset();
        }
        assert!(document.bounded());
    }
}

#[test]
fn final_drag_motion_and_release_in_one_frame_still_pan() {
    let mut document = Document::new(viewport(vec2(390., 400.)));
    document.reset();
    let pointer = document.origin() + document.size() * 0.5;
    let before = document.pan;
    press(&mut document, pointer);
    release(&mut document, pointer + vec2(24., 32.));
    close(document.pan, before + vec2(24., 32.));
    assert_eq!(document.drag_frames, 1);
    assert_eq!(document.hits, 0);
    assert!(!document.captured());
    assert!(document.bounded());
}
