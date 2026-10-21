//! Shared native/WebGL checks. Pixel coordinates below are top-left based.
use macroquad::prelude::*;

fn pixel(image: &Image, x: u32, y: u32) -> &[u8] {
    let i = (((u32::from(image.height) - 1 - y) * u32::from(image.width) + x) * 4) as usize;
    &image.bytes[i..i + 4]
}

fn check(image: &Image, expected: impl Fn(u32, u32) -> [u8; 4]) -> u32 {
    for y in 0..u32::from(image.height) {
        for x in 0..u32::from(image.width) {
            let actual = pixel(image, x, y);
            let expected = expected(x, y);
            assert!(
                actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
                "pixel ({x},{y}): {actual:?}, expected {expected:?}"
            );
        }
    }
    u32::from(image.width) * u32::from(image.height)
}

fn read(target: &RenderTarget) -> Image {
    unsafe { get_internal_gl().flush() };
    target.texture.get_texture_data()
}

fn fill(color: Color) {
    draw_rectangle(-1000., -1000., 3000., 3000., color);
}

/// Runs on the default framebuffer and an asymmetrical 96 x 80 target.
pub async fn run() -> u32 {
    let mut checked = 0;
    set_default_camera();
    clear_background(BLACK);
    with_clip_rect(Rect::new(10.25, 15.25, 30.5, 20.5), || {
        fill(Color::new(1., 0., 0., 1.));
        with_clip_rect(Rect::new(22., 23., 100., 100.), || {
            fill(Color::new(0., 0., 1., 1.))
        });
        // These two logical rectangles are disjoint even though outward
        // rounding at DPR 1 or 2 can give them the same pixel mask.
        with_clip_rect(Rect::new(11.1, 16.1, 0.1, 0.1), || {
            with_clip_rect(Rect::new(11.3, 16.3, 0.1, 0.1), || fill(GREEN));
        });
    });
    draw_rectangle(50., 5., 8., 8., Color::new(1., 1., 0., 1.));
    let dpi = screen_dpi_scale();
    let image = get_screen_data();
    checked += check(&image, |x, y| {
        let xf = x as f32;
        let yf = y as f32;
        if xf >= 50. * dpi && xf < 58. * dpi && yf >= 5. * dpi && yf < 13. * dpi {
            return [255, 255, 0, 255];
        }
        if xf >= (10.25 * dpi).floor()
            && xf < (40.75 * dpi).ceil()
            && yf >= (15.25 * dpi).floor()
            && yf < (35.75 * dpi).ceil()
        {
            if xf >= 22. * dpi && yf >= 23. * dpi {
                [0, 0, 255, 255]
            } else {
                [255, 0, 0, 255]
            }
        } else {
            [0, 0, 0, 255]
        }
    });

    let target = render_target(96, 80);
    let camera = Camera2D {
        render_target: Some(target.clone()),
        ..Camera2D::from_display_rect(Rect::new(0., 0., 96., 80.))
    };
    set_camera(&camera);
    clear_background(BLACK);
    let value = with_clip_rect(Rect::new(10.25, 9.5, 50., 40.), || {
        fill(Color::new(1., 0., 0., 0.5));
        with_clip_rect(Rect::new(30., 20., 60., 40.), || {
            let texture = Texture2D::from_rgba8(1, 1, &[0, 0, 255, 255]);
            draw_texture_ex(
                &texture,
                0.,
                0.,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(96., 80.)),
                    ..Default::default()
                },
            );
            // The queued draw must retain this texture after the local owner drops.
        });
        with_clip_rect(Rect::new(0., 0., 5., 5.), || {
            fill(GREEN);
            with_clip_rect(Rect::new(10., 10., 90., 90.), || fill(WHITE));
        });
        draw_rectangle(12., 12., 4., 4., Color::new(0., 1., 0., 1.));
        17
    });
    assert_eq!(value, 17);
    draw_rectangle(80., 4., 8., 8., Color::new(1., 1., 0., 1.));
    checked += check(&read(&target), |x, y| {
        if (80..88).contains(&x) && (4..12).contains(&y) {
            [255, 255, 0, 255]
        } else if (12..16).contains(&x) && (12..16).contains(&y) {
            [0, 255, 0, 255]
        } else if (30..61).contains(&x) && (20..50).contains(&y) {
            [0, 0, 255, 255]
        } else if (10..61).contains(&x) && (9..50).contains(&y) {
            [128, 0, 0, 255]
        } else {
            [0, 0, 0, 255]
        }
    });

    // Invalid clips fail closed, and cannot corrupt surrounding queued draws.
    clear_background(BLACK);
    draw_rectangle(0., 0., 4., 4., Color::new(0., 1., 0., 1.));
    for rect in [
        Rect::new(0., 0., 0., 10.),
        Rect::new(0., 0., 10., -1.),
        Rect::new(f32::NAN, 0., 10., 10.),
        Rect::new(0., 0., f32::INFINITY, 10.),
    ] {
        with_clip_rect(rect, || fill(RED));
    }
    checked += check(&read(&target), |x, y| {
        if x < 4 && y < 4 {
            [0, 255, 0, 255]
        } else {
            [0, 0, 0, 255]
        }
    });

    // Text is clipped by the same path; baseline and full glyphs extend outside.
    clear_background(BLACK);
    with_clip_rect(Rect::new(20., 10., 20., 30.), || {
        draw_text("MMMMMMMM", 0., 30., 30., WHITE);
    });
    let image = read(&target);
    let mut glyph_pixels = 0;
    for y in 0..80 {
        for x in 0..96 {
            let p = pixel(&image, x, y);
            if (20..40).contains(&x) && (10..40).contains(&y) {
                glyph_pixels += u32::from(p[0] > 0);
            } else {
                assert_eq!(p, &[0, 0, 0, 255], "text escaped clip ({x},{y})");
            }
            assert_eq!(p[3], 255);
            checked += 1;
        }
    }
    assert!(glyph_pixels > 20, "clipped glyphs must draw");

    // GL viewport coordinates are physical and bottom-left based. The clip
    // stays absolute to this target's top-left, with no window DPI multiplier.
    set_camera(&Camera2D {
        viewport: Some((7, 11, 60, 45)),
        render_target: Some(target.clone()),
        ..Camera2D::from_display_rect(Rect::new(0., 0., 96., 80.))
    });
    clear_background(BLACK);
    with_clip_rect(Rect::new(3., 20., 30., 40.), || {
        fill(Color::new(1., 0., 0., 1.))
    });
    checked += check(&read(&target), |x, y| {
        if (7..33).contains(&x) && (24..60).contains(&y) {
            [255, 0, 0, 255]
        } else {
            [0, 0, 0, 255]
        }
    });
    set_camera(&camera);

    // A pre-existing low-level clip is preserved, rather than overwritten by
    // entry/exit. Only the fixture uses internal GL to arrange this legacy case.
    clear_background(BLACK);
    unsafe { get_internal_gl().quad_gl.scissor(Some((3, 4, 20, 20))) };
    with_clip_rect(Rect::new(10., 10., 30., 30.), || {
        fill(Color::new(1., 0., 0., 1.))
    });
    draw_rectangle(3., 4., 3., 3., Color::new(0., 1., 0., 1.));
    unsafe { get_internal_gl().quad_gl.scissor(None) };
    checked += check(&read(&target), |x, y| {
        if (3..6).contains(&x) && (4..7).contains(&y) {
            [0, 255, 0, 255]
        } else if (10..23).contains(&x) && (10..24).contains(&y) {
            [255, 0, 0, 255]
        } else {
            [0, 0, 0, 255]
        }
    });

    // Native unwinding restores both the exact parent and raw renderer clip.
    #[cfg(not(target_arch = "wasm32"))]
    {
        clear_background(BLACK);
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let result = std::panic::catch_unwind(|| {
            with_clip_rect(Rect::new(0., 0., 1., 1.), || panic!("fixture unwind"))
        });
        std::panic::set_hook(hook);
        assert!(result.is_err());
        draw_rectangle(80., 70., 4., 4., Color::new(0., 1., 0., 1.));
        checked += check(&read(&target), |x, y| {
            if (80..84).contains(&x) && (70..74).contains(&y) {
                [0, 255, 0, 255]
            } else {
                [0, 0, 0, 255]
            }
        });
    }

    // Deferred built-in UI uses its own clip even when queued in an empty
    // immediate scope. An offscreen attachment survives next_frame readback.
    let (width, height) = miniquad::window::screen_size();
    let ui_target = render_target(width as u32, height as u32);
    set_camera(&Camera2D {
        render_target: Some(ui_target.clone()),
        ..Camera2D::from_display_rect(Rect::new(0., 0., screen_width(), screen_height()))
    });
    clear_background(WHITE);
    with_clip_rect(Rect::new(0., 0., 0., 0.), || {
        macroquad::ui::root_ui().label(Some(vec2(20., 20.)), "Deferred UI remains visible");
    });
    next_frame().await;
    let image = ui_target.texture.get_texture_data();
    assert!(
        image.bytes.chunks(4).any(|p| p[0] < 200),
        "deferred UI inherited empty immediate clip"
    );
    assert!(image.bytes.chunks(4).all(|p| p[3] == 255));
    checked += u32::from(image.width) * u32::from(image.height);
    set_default_camera();
    checked
}
