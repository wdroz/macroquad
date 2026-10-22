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
fn control_horizontal_wheel_keeps_fit_following_resize() {
    let mut document = Document::new(viewport(vec2(390., 400.)));
    let pointer = document.origin() + document.size() * 0.5;
    let before = (document.zoom, document.pan);
    assert!(!document.update(Input {
        pointer,
        wheel: vec2(3., 0.),
        zoom_modifier: true,
        ..Default::default()
    }));
    assert_eq!((document.zoom, document.pan), before);
    assert!(document.follow_fit);
    assert_eq!((document.wheels, document.zooms), (0, 0));
    document.resize(viewport(vec2(960., 640.)));
    assert_eq!(document.zoom, document.fit_zoom());
    assert!(document.follow_fit);
}

#[test]
fn control_horizontal_wheel_does_not_cancel_a_captured_drag() {
    let mut document = Document::new(viewport(vec2(390., 400.)));
    document.reset();
    let pointer = document.origin() + document.size() * 0.5;
    let before = document.pan;
    press(&mut document, pointer);
    assert!(!document.update(Input {
        pointer,
        down: true,
        wheel: vec2(-3., 0.),
        zoom_modifier: true,
        ..Default::default()
    }));
    assert!(document.captured());
    release(&mut document, pointer + vec2(24., 32.));
    close(document.pan, before + vec2(24., 32.));
    assert_eq!(document.drag_frames, 1);
    assert!(!document.captured());
    assert!(document.bounded());
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

fn touch(id: u64, phase: TouchPhase, position: Vec2) -> Touch {
    Touch {
        id,
        phase,
        position,
    }
}

fn touch_frame(document: &mut Document, contacts: &[Touch]) {
    document.update_with_touches(Input::default(), contacts);
    assert!(document.bounded());
}

fn manual_document() -> Document {
    let mut document = Document::new(viewport(vec2(390., 400.)));
    document.reset();
    document
}

fn center(document: &Document) -> Vec2 {
    document.origin() + document.size() * 0.5
}

#[test]
fn touch_taps_have_jitter_threshold_and_final_release_motion() {
    let mut document = manual_document();
    let note = document.page_to_window(vec2(300., 300.));
    let before = document.pan;
    touch_frame(&mut document, &[touch(7, TouchPhase::Started, note)]);
    touch_frame(
        &mut document,
        &[touch(7, TouchPhase::Moved, note + vec2(2., 0.))],
    );
    close(document.pan, before);
    touch_frame(
        &mut document,
        &[touch(7, TouchPhase::Ended, note + vec2(2., 0.))],
    );
    assert_eq!((document.hits, document.touch_taps), (1, 1));
    assert!(document.selected);
    assert!(!document.captured());
    touch_frame(&mut document, &[touch(8, TouchPhase::Started, note)]);
    touch_frame(
        &mut document,
        &[touch(8, TouchPhase::Ended, note + vec2(18., 25.))],
    );
    close(document.pan, before + vec2(18., 25.));
    assert_eq!(
        (document.hits, document.touch_taps, document.drag_frames),
        (1, 1, 1)
    );
}

#[test]
fn touch_only_starts_inside_acquire_and_second_outside_never_joins() {
    let mut document = manual_document();
    let inside = center(&document);
    let outside = vec2(20., 40.);
    let before = document.pan;
    touch_frame(&mut document, &[touch(4, TouchPhase::Started, outside)]);
    touch_frame(&mut document, &[touch(4, TouchPhase::Moved, inside)]);
    assert!(!document.captured());
    close(document.pan, before);
    touch_frame(
        &mut document,
        &[
            touch(4, TouchPhase::Stationary, inside),
            touch(9, TouchPhase::Started, inside),
            touch(2, TouchPhase::Started, outside),
        ],
    );
    assert_eq!(document.touch_ids(), [Some(9), None]);
    touch_frame(
        &mut document,
        &[
            touch(2, TouchPhase::Moved, inside + vec2(20., 0.)),
            touch(9, TouchPhase::Moved, inside + vec2(12., 16.)),
            touch(4, TouchPhase::Moved, inside + vec2(40., 0.)),
        ],
    );
    assert_eq!(document.touch_mode(), 1);
    close(document.pan, before + vec2(12., 16.));
    assert_eq!(document.rejected_touch_starts, 2);
}

#[test]
fn captured_touch_crosses_viewport_edges_without_losing_id_or_clicking() {
    let mut document = manual_document();
    let inside = center(&document);
    touch_frame(&mut document, &[touch(42, TouchPhase::Started, inside)]);
    for position in [vec2(-10000., -10000.), vec2(10000., 10000.)] {
        touch_frame(&mut document, &[touch(42, TouchPhase::Moved, position)]);
        assert_eq!(document.touch_ids(), [Some(42), None]);
    }
    close(document.pan, Vec2::ZERO);
    touch_frame(&mut document, &[touch(42, TouchPhase::Ended, inside)]);
    assert_eq!(document.hits, 0);
    assert!(!document.captured());
}

#[test]
fn simultaneous_starts_and_reordered_arrays_preserve_exact_u64_owners() {
    let mut first = manual_document();
    let mut reordered = first.clone();
    let p = center(&first);
    let ids = [u64::MAX - 10, u64::MAX - 9, u64::MAX - 8];
    let a = touch(ids[0], TouchPhase::Started, p - vec2(40., 0.));
    let b = touch(ids[1], TouchPhase::Started, p + vec2(40., 0.));
    let extra = touch(ids[2], TouchPhase::Started, p);
    touch_frame(&mut first, &[extra.clone(), a.clone(), b.clone()]);
    touch_frame(&mut reordered, &[b, extra, a]);
    assert_eq!(first.touch_ids(), [Some(ids[0]), Some(ids[1])]);
    assert_eq!(first.touch_ids(), reordered.touch_ids());
    let a = touch(ids[0], TouchPhase::Moved, p - vec2(55., 0.) + vec2(4., 8.));
    let b = touch(ids[1], TouchPhase::Moved, p + vec2(55., 0.) + vec2(4., 8.));
    let extra = touch(ids[2], TouchPhase::Moved, p + vec2(10000., -10000.));
    touch_frame(&mut first, &[extra.clone(), b.clone(), a.clone()]);
    touch_frame(&mut reordered, &[a, extra, b]);
    assert_eq!(first.zoom, reordered.zoom);
    close(first.pan, reordered.pan);
    assert_eq!(first.rejected_touch_starts, 1);
}

#[test]
fn pinch_scales_about_previous_midpoint_and_follows_its_translation() {
    let mut document = manual_document();
    let p = center(&document);
    let a = p - vec2(40., 10.);
    let b = p + vec2(40., 10.);
    let page = document.window_to_page(p);
    let before = document.pan;
    touch_frame(&mut document, &[touch(8, TouchPhase::Started, a)]);
    // The first finger also moved in this transition snapshot: neither that
    // move nor the new midpoint may jump the existing view.
    touch_frame(
        &mut document,
        &[
            touch(8, TouchPhase::Moved, a + vec2(4., 0.)),
            touch(3, TouchPhase::Started, b - vec2(4., 0.)),
        ],
    );
    close(document.pan, before);
    assert_eq!(document.zoom, 1.);
    assert_eq!(document.touch_ids(), [Some(8), Some(3)]);
    let delta = vec2(10., 16.);
    touch_frame(
        &mut document,
        &[
            touch(
                3,
                TouchPhase::Moved,
                p + (b - vec2(4., 0.) - p) * 1.5 + delta,
            ),
            touch(
                8,
                TouchPhase::Moved,
                p + (a + vec2(4., 0.) - p) * 1.5 + delta,
            ),
        ],
    );
    assert!((document.zoom - 1.5).abs() < 0.00001);
    close(document.page_to_window(page), p + delta);
    assert!(!document.anchor_clamped);
    assert!(document.anchor_error < 0.002);
    assert_eq!((document.pinch_starts, document.pinch_frames), (1, 1));
}

#[test]
fn touch_pinch_bounds_and_clamping_report_anchor_adjustments() {
    let mut document = Document::new(viewport(vec2(960., 640.)));
    let p = document.origin() + vec2(30., 20.);
    touch_frame(
        &mut document,
        &[
            touch(1, TouchPhase::Started, p - vec2(10., 0.)),
            touch(2, TouchPhase::Started, p + vec2(10., 0.)),
        ],
    );
    touch_frame(
        &mut document,
        &[
            touch(1, TouchPhase::Moved, p - vec2(12., 0.)),
            touch(2, TouchPhase::Moved, p + vec2(12., 0.)),
        ],
    );
    assert!(document.anchor_clamped);
    assert!(document.anchor_error > 1.);
    for distance in [10000., 2., 10000., 2.] {
        touch_frame(
            &mut document,
            &[
                touch(2, TouchPhase::Moved, p + vec2(distance, 0.)),
                touch(1, TouchPhase::Moved, p - vec2(distance, 0.)),
            ],
        );
        assert!(document.zoom >= document.fit_zoom().min(0.2));
        assert!(document.zoom <= state::MAX_ZOOM);
    }
}

#[test]
fn pair_loss_rebases_survivor_without_tap_or_adopting_ignored_third_finger() {
    for terminal in [TouchPhase::Ended, TouchPhase::Cancelled] {
        let mut document = manual_document();
        let p = center(&document);
        let before = document.pan;
        touch_frame(
            &mut document,
            &[
                touch(11, TouchPhase::Started, p - vec2(30., 0.)),
                touch(22, TouchPhase::Started, p + vec2(30., 0.)),
                touch(33, TouchPhase::Started, p),
            ],
        );
        touch_frame(
            &mut document,
            &[
                touch(33, TouchPhase::Stationary, p),
                touch(22, terminal, p + vec2(9000., 9000.)),
                touch(11, TouchPhase::Moved, p),
            ],
        );
        close(document.pan, before);
        assert_eq!(document.touch_ids(), [Some(11), None]);
        touch_frame(
            &mut document,
            &[
                touch(11, TouchPhase::Moved, p + vec2(12., 8.)),
                touch(33, TouchPhase::Moved, p + vec2(100., 0.)),
            ],
        );
        close(document.pan, before + vec2(12., 8.));
        touch_frame(
            &mut document,
            &[touch(11, TouchPhase::Ended, p + vec2(12., 8.))],
        );
        assert_eq!(document.hits, 0);
        assert!(!document.captured());
    }
}

#[test]
fn either_pinch_owner_can_survive_and_fresh_second_start_rejoins() {
    let mut document = manual_document();
    let p = center(&document);
    touch_frame(
        &mut document,
        &[
            touch(5, TouchPhase::Started, p - vec2(30., 0.)),
            touch(9, TouchPhase::Started, p + vec2(30., 0.)),
        ],
    );
    touch_frame(
        &mut document,
        &[
            touch(5, TouchPhase::Ended, p - vec2(30., 0.)),
            touch(9, TouchPhase::Stationary, p + vec2(30., 0.)),
        ],
    );
    assert_eq!(document.touch_ids(), [Some(9), None]);
    let before = (document.pan, document.zoom);
    touch_frame(
        &mut document,
        &[
            touch(2, TouchPhase::Started, p - vec2(40., 0.)),
            touch(9, TouchPhase::Moved, p + vec2(40., 0.)),
        ],
    );
    assert_eq!(document.touch_ids(), [Some(9), Some(2)]);
    assert_eq!((document.pan, document.zoom), before);
    assert_eq!(document.pinch_starts, 2);
}

#[test]
fn captured_drag_outside_can_join_a_second_start_inside() {
    let mut document = manual_document();
    let p = center(&document);
    touch_frame(&mut document, &[touch(1, TouchPhase::Started, p)]);
    touch_frame(
        &mut document,
        &[touch(1, TouchPhase::Moved, vec2(-50., 220.))],
    );
    let before = (document.pan, document.zoom);
    touch_frame(
        &mut document,
        &[
            touch(1, TouchPhase::Stationary, vec2(-50., 220.)),
            touch(2, TouchPhase::Started, p),
        ],
    );
    assert_eq!(document.touch_ids(), [Some(1), Some(2)]);
    assert_eq!((document.pan, document.zoom), before);
}

#[test]
fn coincident_and_subpixel_fingers_translate_then_rebase_scale() {
    let mut document = manual_document();
    let p = center(&document);
    let page = document.window_to_page(p);
    touch_frame(
        &mut document,
        &[
            touch(1, TouchPhase::Started, p),
            touch(2, TouchPhase::Started, p),
        ],
    );
    for (distance, delta) in [(0.5, vec2(3., 4.)), (20., vec2(6., 8.))] {
        touch_frame(
            &mut document,
            &[
                touch(1, TouchPhase::Moved, p + delta - vec2(distance * 0.5, 0.)),
                touch(2, TouchPhase::Moved, p + delta + vec2(distance * 0.5, 0.)),
            ],
        );
        assert_eq!(document.zoom, 1.);
        close(document.page_to_window(page), p + delta);
    }
    touch_frame(
        &mut document,
        &[
            touch(1, TouchPhase::Moved, p + vec2(6., 8.) - vec2(20., 0.)),
            touch(2, TouchPhase::Moved, p + vec2(6., 8.) + vec2(20., 0.)),
        ],
    );
    assert_eq!(document.zoom, 2.);
    close(document.page_to_window(page), p + vec2(6., 8.));
}

#[test]
fn cancellation_missing_contacts_and_nonfinite_positions_cannot_select() {
    for terminal in [
        Some(touch(1, TouchPhase::Cancelled, vec2(195., 228.))),
        Some(touch(1, TouchPhase::Ended, vec2(f32::NAN, 228.))),
        Some(touch(1, TouchPhase::Moved, vec2(195., f32::INFINITY))),
        None,
    ] {
        let mut document = manual_document();
        let p = document.page_to_window(vec2(300., 300.));
        let before = (document.pan, document.zoom);
        touch_frame(&mut document, &[touch(1, TouchPhase::Started, p)]);
        touch_frame(&mut document, &terminal.into_iter().collect::<Vec<_>>());
        assert!(!document.captured());
        assert_eq!((document.pan, document.zoom), before);
        assert_eq!((document.hits, document.touch_cancels), (0, 1));
    }
    let mut document = manual_document();
    let p = center(&document);
    touch_frame(
        &mut document,
        &[
            touch(1, TouchPhase::Started, p - vec2(20., 0.)),
            touch(2, TouchPhase::Started, p + vec2(20., 0.)),
        ],
    );
    let before = (document.pan, document.zoom);
    touch_frame(&mut document, &[touch(2, TouchPhase::Stationary, p)]);
    assert_eq!(document.touch_ids(), [Some(2), None]);
    assert_eq!((document.pan, document.zoom), before);
    touch_frame(&mut document, &[]);
    assert!(!document.captured());
    touch_frame(
        &mut document,
        &[
            touch(3, TouchPhase::Started, p - vec2(20., 0.)),
            touch(4, TouchPhase::Started, p + vec2(20., 0.)),
        ],
    );
    let cancels = document.touch_cancels;
    touch_frame(
        &mut document,
        &[
            touch(3, TouchPhase::Moved, vec2(f32::NAN, p.y)),
            touch(4, TouchPhase::Stationary, p),
        ],
    );
    assert_eq!(document.touch_ids(), [Some(4), None]);
    assert_eq!(document.touch_cancels, cancels + 1);
}

#[test]
fn completed_between_frames_or_unowned_terminal_contacts_do_not_acquire() {
    let mut document = manual_document();
    let p = document.page_to_window(vec2(300., 300.));
    for phase in [
        TouchPhase::Ended,
        TouchPhase::Cancelled,
        TouchPhase::Moved,
        TouchPhase::Stationary,
    ] {
        touch_frame(&mut document, &[touch(1, phase, p)]);
        assert!(!document.captured());
    }
    assert_eq!((document.hits, document.touch_starts), (0, 0));
}

#[test]
fn touch_suppresses_queued_mouse_wheel_and_buttons_until_neutral_frame() {
    let mut document = manual_document();
    let p = center(&document);
    press(&mut document, p);
    let queued = Input {
        pointer: p,
        pressed: true,
        down: true,
        released: true,
        wheel: vec2(-2., 3.),
        zoom_modifier: true,
    };
    let before = (
        document.pan,
        document.zoom,
        document.hits,
        document.inside_presses,
    );
    document.update_with_touches(queued, &[touch(2, TouchPhase::Started, p)]);
    assert!(!document.mouse_allowed());
    document.update_with_touches(queued, &[touch(2, TouchPhase::Cancelled, p)]);
    assert!(!document.mouse_allowed());
    document.update(queued);
    assert!(!document.mouse_allowed());
    assert_eq!(
        (
            document.pan,
            document.zoom,
            document.hits,
            document.inside_presses
        ),
        before
    );
    assert!(!document.captured());
    document.update(Input::default());
    assert!(document.mouse_allowed());
    let note = document.page_to_window(vec2(300., 300.));
    press(&mut document, note);
    release(&mut document, note);
    assert_eq!(document.hits, 1);
    assert!(document.bounded());
}

#[test]
fn reset_fit_and_resize_cancel_touch_ownership_until_fresh_started_phase() {
    for action in 0..3 {
        let mut document = manual_document();
        let p = center(&document);
        touch_frame(
            &mut document,
            &[
                touch(1, TouchPhase::Started, p - vec2(25., 0.)),
                touch(2, TouchPhase::Started, p + vec2(25., 0.)),
            ],
        );
        match action {
            0 => document.reset(),
            1 => document.fit(),
            _ => {
                document.resize(viewport(vec2(960., 640.)));
            }
        }
        assert!(!document.captured());
        let before = (document.pan, document.zoom);
        touch_frame(
            &mut document,
            &[
                touch(1, TouchPhase::Moved, p - vec2(50., 0.)),
                touch(2, TouchPhase::Moved, p + vec2(50., 0.)),
            ],
        );
        assert_eq!((document.pan, document.zoom), before);
        assert!(!document.captured());
        touch_frame(&mut document, &[]);
        let fresh = center(&document);
        touch_frame(&mut document, &[touch(7, TouchPhase::Started, fresh)]);
        assert_eq!(document.touch_ids(), [Some(7), None]);
    }
}
