//! Actual pixel coverage, shared by native OpenGL and the WebGL example.
use macroquad::prelude::*;

const SIZE: u32 = 128;

fn read(target: &RenderTarget) -> Image {
    unsafe { get_internal_gl().flush() };
    target.texture.get_texture_data()
}

fn draw(radius: f32) {
    draw_rectangle_rounded(8., 8., 112., 112., radius, Color::new(1., 0., 0., 0.5));
}

fn close(actual: u8, expected: u8) {
    assert!(
        actual.abs_diff(expected) <= 1,
        "expected {expected}, got {actual}"
    );
}

/// Returns the number of pixels checked for single coverage.
pub fn run() -> u32 {
    let target = render_target(SIZE, SIZE);
    set_camera(&Camera2D {
        render_target: Some(target.clone()),
        ..Camera2D::from_display_rect(Rect::new(0., 0., SIZE as f32, SIZE as f32))
    });
    let mut checked = 0;
    for height in [112_u32, 48] {
        let top = (SIZE - height) / 2;
        let bottom = top + height;
        for radius in [0_f32, 1., 8., 23.25, 56., 1000.] {
            for opaque in [false, true] {
                clear_background(if opaque { BLACK } else { BLANK });
                draw_rectangle_rounded(
                    8.,
                    top as f32,
                    112.,
                    height as f32,
                    radius,
                    Color::new(1., 0., 0., 0.5),
                );
                let image = read(&target);
                let radius = radius.min(height as f32 * 0.5);
                let mut lit = 0;
                for y in 0..SIZE {
                    for x in 0..SIZE {
                        let pixel = &image.bytes[((y * SIZE + x) * 4) as usize..][..4];
                        // Symmetric target/shape makes readback Y orientation immaterial.
                        // Points over 0.5 px inside the analytic arc must be filled.
                        // The chord approximation may remove only near-edge pixels.
                        let point = vec2(x as f32 + 0.5, y as f32 + 0.5);
                        let nearest = point.clamp(
                            vec2(8. + radius, top as f32 + radius),
                            vec2(120. - radius, bottom as f32 - radius),
                        );
                        let inside_box = (8..120).contains(&x) && (top..bottom).contains(&y);
                        let safe_inside =
                            inside_box && (radius == 0. || point.distance(nearest) < radius - 0.5);
                        let outside = !inside_box || point.distance(nearest) > radius + 0.05;
                        if safe_inside {
                            close(pixel[0], 128);
                        }
                        if outside {
                            assert_eq!(pixel[0], 0, "outside radius {radius}: ({x},{y}) {pixel:?}");
                        }
                        if pixel[0] > 0 {
                            // Every covered pixel has exactly one translucent layer:
                            // neither overlapping triangles nor darker shared seams.
                            close(pixel[0], 128);
                            assert_eq!(&pixel[1..3], &[0, 0]);
                            close(pixel[3], if opaque { 255 } else { 128 });
                            lit += 1;
                        } else {
                            assert_eq!(pixel[3], if opaque { 255 } else { 0 });
                        }
                        checked += 1;
                    }
                }
                assert!(
                    lit > 112 * height * 7 / 10,
                    "empty or incomplete rounded panel"
                );
            }
        }
    }

    clear_background(BLANK);
    draw(56.);
    let clamped = read(&target).bytes;
    clear_background(BLANK);
    draw(1000.);
    assert_eq!(read(&target).bytes, clamped, "oversized radius must clamp");

    clear_background(BLANK);
    draw_rectangle(8., 8., 112., 112., Color::new(1., 0., 0., 0.5));
    let square = read(&target).bytes;
    for radius in [0., -10.] {
        clear_background(BLANK);
        draw(radius);
        assert_eq!(
            read(&target).bytes,
            square,
            "nonpositive radius must be square"
        );
    }

    // Invalid inputs must submit nothing, including while a draw is queued.
    clear_background(BLANK);
    draw_rectangle(0., 0., 4., 4., GREEN);
    for (x, y, w, h, r) in [
        (8., 8., 0., 50., 4.),
        (8., 8., 50., -1., 4.),
        (f32::NAN, 8., 50., 50., 4.),
        (8., f32::INFINITY, 50., 50., 4.),
        (8., 8., f32::INFINITY, 50., 4.),
        (8., 8., 50., 50., f32::NAN),
        (f32::MAX, 8., f32::MAX, 50., 4.),
    ] {
        draw_rectangle_rounded(x, y, w, h, r, RED);
    }
    let image = read(&target);
    assert_eq!(image.bytes.chunks(4).filter(|p| p[1] > 0).count(), 16);
    assert!(image.bytes.chunks(4).all(|p| p[0] == 0));

    // A textured draw before the helper and a normal shape after it check that
    // immediate batching remains ordered and rounded fills reset the sampler.
    let blue = Texture2D::from_rgba8(1, 1, &[0, 0, 255, 255]);
    clear_background(BLACK);
    draw_texture_ex(
        &blue,
        0.,
        0.,
        WHITE,
        DrawTextureParams {
            dest_size: Some(vec2(128., 128.)),
            ..Default::default()
        },
    );
    draw(8.);
    draw_rectangle(60., 60., 8., 8., Color::new(0., 1., 0., 1.));
    let image = read(&target);
    let center = &image.bytes[((64 * SIZE + 64) * 4) as usize..][..4];
    assert_eq!(center, &[0, 255, 0, 255]);
    let panel = &image.bytes[((32 * SIZE + 32) * 4) as usize..][..4];
    close(panel[0], 128);
    close(panel[2], 127);
    assert_eq!(panel[3], 255);
    set_default_camera();
    checked
}
