//! Public coordinate regressions; no window or GPU is initialized.
use macroquad::prelude::*;

fn close(actual: Vec2, expected: Vec2) {
    assert!(
        (actual - expected).length() < 0.0002,
        "actual {actual:?}, expected {expected:?}"
    );
}

#[test]
fn physical_viewport_origin_and_continuous_edges_are_explicit() {
    let camera = Camera2D {
        viewport: Some((20, 30, 200, 100)),
        ..Default::default()
    };
    let size = vec2(640.0, 360.0);
    for (world, pixel) in [
        (Vec2::ZERO, vec2(120.0, 280.0)),
        (vec2(-1.0, -1.0), vec2(20.0, 230.0)),
        (vec2(1.0, 1.0), vec2(220.0, 330.0)),
        (vec2(2.0, -2.0), vec2(320.0, 180.0)),
    ] {
        close(
            camera.world_to_target_with_size(world, size).unwrap(),
            pixel,
        );
        close(
            camera.target_to_world_with_size(pixel, size).unwrap(),
            world,
        );
    }
    // Explicit dimensions work headlessly; automatic methods need a live surface.
    assert!(camera.world_to_target(Vec2::ZERO).is_none());
    assert!(camera.target_to_world(Vec2::ZERO).is_none());
}

#[test]
fn rotated_camera_round_trips_after_surface_resizing() {
    let mut camera = Camera2D {
        rotation: 37.0,
        target: vec2(25.0, -14.0),
        zoom: vec2(-0.013, 0.021),
        offset: vec2(0.2, -0.3),
        ..Default::default()
    };
    for viewport in [None, Some((27, 19, 240, 140)), Some((-30, -20, 420, 240))] {
        camera.viewport = viewport;
        for size in [vec2(320.0, 180.0), vec2(640.0, 360.0), vec2(390.0, 400.0)] {
            for point in [
                Vec2::ZERO,
                camera.target,
                vec2(-21.0, 49.0),
                vec2(86.0, -72.0),
            ] {
                let pixel = camera.world_to_target_with_size(point, size).unwrap();
                close(
                    camera.target_to_world_with_size(pixel, size).unwrap(),
                    point,
                );
            }
        }
    }
}

#[test]
fn physical_window_projection_scales_with_dpi_and_physical_viewport() {
    let world = vec2(0.32, -0.17);
    let logical_size = vec2(390.0, 400.0);
    let mut logical_pixel = None;
    for dpi in [1, 2] {
        let camera = Camera2D {
            viewport: Some((20 * dpi, 30 * dpi, 300 * dpi, 340 * dpi)),
            rotation: -21.0,
            offset: vec2(0.1, -0.1),
            ..Default::default()
        };
        let physical_size = logical_size * dpi as f32;
        let physical = camera
            .world_to_target_with_size(world, physical_size)
            .unwrap();
        let logical = physical / dpi as f32;
        if let Some(expected) = logical_pixel {
            close(logical, expected);
        }
        logical_pixel = Some(logical);
        close(
            camera
                .target_to_world_with_size(logical * dpi as f32, physical_size)
                .unwrap(),
            world,
        );
    }
}

#[test]
fn display_mapping_accounts_for_raw_target_y_and_both_flips() {
    let size = vec2(320.0, 180.0);
    let display = Rect::new(40.0, 70.0, 640.0, 360.0);
    let input = vec2(200.0, 340.0);
    for flip_x in [false, true] {
        for flip_y in [false, true] {
            let expected = vec2(
                if flip_x { 240.0 } else { 80.0 },
                if flip_y { 135.0 } else { 45.0 },
            );
            let target = window_to_render_target(input, size, display, flip_x, flip_y).unwrap();
            close(target, expected);
            close(
                render_target_to_window(target, size, display, flip_x, flip_y).unwrap(),
                input,
            );
        }
    }
}

#[test]
fn display_boundaries_are_included_but_letterbox_bars_are_rejected() {
    let size = vec2(320.0, 180.0);
    let display = Rect::new(10.0, 90.0, 320.0, 180.0);
    for flip_x in [false, true] {
        for flip_y in [false, true] {
            for point in [
                vec2(10.0, 90.0),
                vec2(330.0, 90.0),
                vec2(10.0, 270.0),
                vec2(330.0, 270.0),
            ] {
                let target = window_to_render_target(point, size, display, flip_x, flip_y).unwrap();
                close(
                    render_target_to_window(target, size, display, flip_x, flip_y).unwrap(),
                    point,
                );
            }
            for point in [
                vec2(9.0, 100.0),
                vec2(331.0, 100.0),
                vec2(100.0, 89.0),
                vec2(100.0, 271.0),
            ] {
                assert!(window_to_render_target(point, size, display, flip_x, flip_y).is_none());
            }
            for point in [
                vec2(-1.0, 50.0),
                vec2(321.0, 50.0),
                vec2(50.0, -1.0),
                vec2(50.0, 181.0),
            ] {
                assert!(render_target_to_window(point, size, display, flip_x, flip_y).is_none());
            }
        }
    }
}

#[test]
fn texture_mapping_uses_logical_input_independently_of_dpi() {
    let size = vec2(320.0, 180.0);
    let logical_display = Rect::new(35.0, 110.0, 320.0, 180.0);
    let logical_input = vec2(115.0, 155.0);
    for dpi in [1.0, 2.0] {
        // Models mouse_position_logical's physical-to-logical conversion.
        let input = logical_input * dpi / dpi;
        close(
            window_to_render_target(input, size, logical_display, false, true).unwrap(),
            vec2(80.0, 45.0),
        );
    }
}

#[test]
fn fractional_display_edges_do_not_escape_texture_bounds() {
    let size = vec2(320.0, 180.0);
    let display = Rect::new(0.1, 0.1, 0.2, 0.2);
    let far = vec2(display.x + display.w, display.y + display.h);
    for flip_x in [false, true] {
        for flip_y in [false, true] {
            let pixels = window_to_render_target(far, size, display, flip_x, flip_y).unwrap();
            assert_eq!(
                pixels,
                vec2(
                    if flip_x { 0.0 } else { 320.0 },
                    if flip_y { 180.0 } else { 0.0 }
                )
            );
            assert_eq!(
                render_target_to_window(pixels, size, display, flip_x, flip_y),
                Some(far)
            );
        }
    }
}

#[test]
fn invalid_and_unrepresentable_inputs_return_none() {
    let camera = Camera2D::default();
    let size = vec2(320.0, 180.0);
    let display = Rect::new(0.0, 0.0, 640.0, 360.0);
    for bad_size in [
        Vec2::ZERO,
        vec2(-1.0, 1.0),
        vec2(1.0, f32::INFINITY),
        vec2(f32::NAN, 1.0),
    ] {
        assert!(camera
            .world_to_target_with_size(Vec2::ZERO, bad_size)
            .is_none());
        assert!(camera
            .target_to_world_with_size(Vec2::ZERO, bad_size)
            .is_none());
        assert!(window_to_render_target(Vec2::ZERO, bad_size, display, false, true).is_none());
        assert!(render_target_to_window(Vec2::ZERO, bad_size, display, false, true).is_none());
    }
    for point in [vec2(f32::NAN, 0.0), vec2(0.0, f32::INFINITY)] {
        assert!(camera.world_to_target_with_size(point, size).is_none());
        assert!(camera.target_to_world_with_size(point, size).is_none());
        assert!(window_to_render_target(point, size, display, false, true).is_none());
        assert!(render_target_to_window(point, size, display, false, true).is_none());
    }
    for rect in [
        Rect::new(0.0, 0.0, 0.0, 1.0),
        Rect::new(0.0, 0.0, 1.0, -1.0),
        Rect::new(f32::NAN, 0.0, 1.0, 1.0),
        Rect::new(f32::MAX, 0.0, f32::MAX, 1.0),
        Rect::new(1e30, 0.0, 1.0, 1.0),
    ] {
        assert!(window_to_render_target(Vec2::ZERO, size, rect, false, true).is_none());
        assert!(render_target_to_window(Vec2::ZERO, size, rect, false, true).is_none());
    }
    for zoom in [Vec2::ZERO, vec2(0.0, 1.0), vec2(1.0, f32::INFINITY)] {
        let camera = Camera2D {
            zoom,
            ..Default::default()
        };
        assert!(camera.world_to_target_with_size(Vec2::ZERO, size).is_none());
        assert!(camera.target_to_world_with_size(Vec2::ZERO, size).is_none());
    }
    for viewport in [(0, 0, 0, 1), (0, 0, 1, -1), (i32::MAX, 0, 1, 1)] {
        let camera = Camera2D {
            viewport: Some(viewport),
            ..Default::default()
        };
        assert!(camera.world_to_target_with_size(Vec2::ZERO, size).is_none());
        assert!(camera.target_to_world_with_size(Vec2::ZERO, size).is_none());
    }
    let camera = Camera2D {
        viewport: Some((0, 0, 10, 10)),
        ..Default::default()
    };
    assert!(camera
        .world_to_target_with_size(Vec2::ZERO, vec2(320.0, f32::MAX))
        .is_none());
}

#[test]
fn viewport_extents_do_not_overflow_integer_addition() {
    let camera = Camera2D {
        viewport: Some((i32::MAX, i32::MAX, i32::MAX, i32::MAX)),
        ..Default::default()
    };
    let size = vec2(320.0, 180.0);
    let pixel = camera.world_to_target_with_size(Vec2::ZERO, size).unwrap();
    assert!(pixel.is_finite());
    close(
        camera.target_to_world_with_size(pixel, size).unwrap(),
        Vec2::ZERO,
    );
}
