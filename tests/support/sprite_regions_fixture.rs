//! Rendered pixels are checked against source texels and independent transforms.
use crate::sheet;
use macroquad::experimental::animation::SpriteRegionDrawParams;
use macroquad::prelude::*;
const WIDTH: u32 = 256;
const HEIGHT: u32 = 192;
const ANCHOR: Vec2 = Vec2::new(120., 96.);
const SCALE: Vec2 = Vec2::new(4., 3.);

fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
    let i = (((u32::from(image.height) - 1 - y) * u32::from(image.width) + x) * 4) as usize;
    image.bytes[i..i + 4].try_into().unwrap()
}
fn expected(frame: usize, point: Vec2, turns: u32, flip_x: bool, flip_y: bool) -> [u8; 4] {
    let p = point - ANCHOR;
    let mut p = match turns {
        0 => p,
        1 => vec2(p.y, -p.x),
        2 => -p,
        _ => vec2(-p.y, p.x),
    };
    if flip_x {
        p.x = -p.x;
    }
    if flip_y {
        p.y = -p.y;
    }
    let source = p / SCALE + sheet::PIVOT;
    let color = sheet::frame_pixel(frame, source.x.floor() as i32, source.y.floor() as i32);
    if color[3] == 0 {
        [0, 0, 0, 255]
    } else {
        color
    }
}

pub async fn run() -> u32 {
    let target = render_target(WIDTH, HEIGHT);
    let camera = Camera2D {
        render_target: Some(target.clone()),
        ..Camera2D::from_display_rect(Rect::new(0., 0., WIDTH as f32, HEIGHT as f32))
    };
    let texture = sheet::texture();
    let mut checked = 0;
    for atlas in [false, true] {
        if atlas {
            set_default_filter_mode(FilterMode::Nearest);
            build_textures_atlas();
            next_frame().await;
        }
        for frame in 0..2 {
            for trimmed in [false, true] {
                for flip_x in [false, true] {
                    for flip_y in [false, true] {
                        for turns in 0..4 {
                            set_camera(&camera);
                            clear_background(BLACK);
                            sheet::region(frame, trimmed)
                                .draw(
                                    &texture,
                                    ANCHOR,
                                    WHITE,
                                    SpriteRegionDrawParams {
                                        dest_size: Some(sheet::FRAME_SIZE * SCALE),
                                        rotation: turns as f32 * std::f32::consts::FRAC_PI_2,
                                        flip_x,
                                        flip_y,
                                    },
                                )
                                .unwrap();
                            unsafe { get_internal_gl().flush() };
                            let image = target.texture.get_texture_data();
                            for y in 0..HEIGHT {
                                for x in 0..WIDTH {
                                    let want = expected(
                                        frame,
                                        vec2(x as f32 + 0.5, y as f32 + 0.5),
                                        turns,
                                        flip_x,
                                        flip_y,
                                    );
                                    let actual = pixel(&image, x, y);
                                    assert_eq!(actual, want, "sprite ({x},{y}), atlas={atlas} frame={frame} trimmed={trimmed} flip=({flip_x},{flip_y}) turns={turns}");
                                    checked += 1;
                                }
                            }
                            next_frame().await;
                        }
                    }
                }
            }
        }
    }
    // Both the direct and atlas routes preserve queued texture ownership.
    for atlas in [false, true] {
        let local = sheet::texture();
        if atlas {
            build_textures_atlas();
            next_frame().await;
        }
        set_camera(&camera);
        clear_background(BLACK);
        sheet::region(1, true)
            .draw(
                &local,
                ANCHOR,
                WHITE,
                SpriteRegionDrawParams {
                    dest_size: Some(sheet::FRAME_SIZE * SCALE),
                    ..Default::default()
                },
            )
            .unwrap();
        drop(local);
        // A mismatched sheet is rejected without submitting geometry.
        let wrong = Texture2D::from_rgba8(1, 1, &[255, 0, 0, 255]);
        assert!(sheet::region(0, true)
            .draw(&wrong, ANCHOR, WHITE, Default::default())
            .is_err());
        draw_rectangle(3., 3., 6., 6., WHITE);
        unsafe { get_internal_gl().flush() };
        let image = target.texture.get_texture_data();
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let want = if (3..9).contains(&x) && (3..9).contains(&y) {
                    [255; 4]
                } else {
                    expected(1, vec2(x as f32 + 0.5, y as f32 + 0.5), 0, false, false)
                };
                assert_eq!(
                    pixel(&image, x, y),
                    want,
                    "queued ownership atlas={atlas} ({x},{y})"
                );
                checked += 1;
            }
        }
        next_frame().await;
    }
    set_default_filter_mode(FilterMode::Linear);
    set_default_camera();
    next_frame().await;
    checked
}
