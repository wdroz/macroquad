//! Shared native/WebGL raster checks, using independently drawn fill masks.
use macroquad::prelude::*;

fn read(target: &RenderTarget) -> Image {
    unsafe { get_internal_gl().flush() };
    target.texture.get_texture_data()
}

pub fn run() -> u32 {
    let target = render_target(128, 128);
    let mut checked = 0;
    for transformed in [false, true] {
        let mut camera = Camera2D {
            render_target: Some(target.clone()),
            ..Camera2D::from_display_rect(Rect::new(0., 0., 128., 128.))
        };
        if transformed {
            camera.rotation = 17.;
            camera.target += vec2(3., -5.);
            camera.zoom *= 0.85;
        }
        set_camera(&camera);
        for height in [48_f32, 112.] {
            let top = (128. - height) * 0.5;
            for radius in [-10_f32, 0., 1., 8., 23.25, 1000.] {
                for thickness in [0_f32, 0.5, 3., 12., 24., 1000.] {
                    let r = radius.max(0.).min(height * 0.5);
                    let t = thickness.min(height * 0.5);
                    // Capture each contour separately, avoiding any outline implementation.
                    let mut masks = Vec::new();
                    for inner in [false, true] {
                        clear_background(BLANK);
                        if !inner || (t > 0. && t < height * 0.5) {
                            let inset = if inner { t } else { 0. };
                            with_clip_rect(Rect::new(0., 0., 102., 128.), || {
                                draw_rectangle_rounded(
                                    8. + inset,
                                    top + inset,
                                    112. - 2. * inset,
                                    height - 2. * inset,
                                    (r - inset).max(0.),
                                    WHITE,
                                );
                            });
                        }
                        masks.push(read(&target));
                    }
                    for opaque in [false, true] {
                        clear_background(if opaque { BLACK } else { BLANK });
                        with_clip_rect(Rect::new(0., 0., 102., 128.), || {
                            with_clip_rect(Rect::new(0., 0., 128., 128.), || {
                                draw_rectangle_rounded_lines(
                                    8.,
                                    top,
                                    112.,
                                    height,
                                    radius,
                                    thickness,
                                    Color::new(1., 0., 0., 0.5),
                                );
                            });
                            with_clip_rect(Rect::new(200., 200., 1., 1.), || {
                                draw_rectangle_rounded_lines(0., 0., 128., 128., 0., 1000., GREEN);
                            });
                        });
                        let image = read(&target);
                        for i in 0..image.bytes.len() / 4 {
                            let pixel = &image.bytes[i * 4..i * 4 + 4];
                            let covered = thickness > 0.
                                && masks[0].bytes[i * 4] > 0
                                && masks[1].bytes[i * 4] == 0;
                            let expected = [
                                if covered { 128 } else { 0 },
                                0,
                                0,
                                if opaque {
                                    255
                                } else if covered {
                                    128
                                } else {
                                    0
                                },
                            ];
                            assert!(pixel.iter().zip(expected).all(|(a,b)|a.abs_diff(b)<=1),
                                "outline {transformed} h={height} r={radius} t={thickness} pixel {i}: {pixel:?}, expected {expected:?}");
                            checked += 1;
                        }
                    }
                }
            }
        }
    }
    // Invalid calls preserve queued textured draws; a later shape is untextured.
    set_camera(&Camera2D {
        render_target: Some(target.clone()),
        ..Camera2D::from_display_rect(Rect::new(0., 0., 128., 128.))
    });
    clear_background(BLANK);
    let blue = Texture2D::from_rgba8(1, 1, &[0, 0, 255, 255]);
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
    for t in [0., -1., f32::NAN, f32::INFINITY] {
        draw_rectangle_rounded_lines(0., 0., 128., 128., 8., t, RED);
    }
    draw_rectangle_rounded_lines(8., 8., 112., 112., 8., 3., Color::new(1., 0., 0., 0.5));
    draw_rectangle(60., 60., 8., 8., Color::new(0., 1., 0., 1.));
    let image = read(&target);
    assert_eq!(&image.bytes[(64 * 128 + 64) * 4..][..4], &[0, 255, 0, 255]);
    let edge = &image.bytes[(64 * 128 + 9) * 4..][..4];
    assert!(edge[0].abs_diff(128) <= 1 && edge[2].abs_diff(127) <= 1);
    assert_eq!(&image.bytes[(64 * 128 + 32) * 4..][..4], &[0, 0, 255, 255]);
    set_default_camera();
    checked
}
