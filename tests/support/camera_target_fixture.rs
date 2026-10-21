//! Pixel checks use actual rendered colors, not just mutually inverse transforms.
use macroquad::camera::{render_target_to_window, window_to_render_target};
use macroquad::prelude::*;

const SIZE: Vec2 = Vec2::new(320., 180.);
const MARKERS: [(Vec2, [u8; 4]); 3] = [
    (Vec2::new(110., 57.), [255, 0, 0, 255]),
    (Vec2::new(202., 65.), [0, 255, 0, 255]),
    (Vec2::new(177., 126.), [0, 0, 255, 255]),
];

fn pixel(image: &Image, point: Vec2) -> [u8; 4] {
    let x = point.x.floor() as u32;
    let y = point.y.floor() as u32;
    assert!(x < u32::from(image.width) && y < u32::from(image.height));
    let i = (((u32::from(image.height) - 1 - y) * u32::from(image.width) + x) * 4) as usize;
    image.bytes[i..i + 4].try_into().unwrap()
}

fn draw_markers() {
    for (point, bytes) in MARKERS {
        let texture = Texture2D::from_rgba8(1, 1, &bytes);
        draw_texture_ex(
            &texture,
            point.x - 10.,
            point.y - 10.,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(20., 20.)),
                ..Default::default()
            },
        );
        // Queued sprite ownership must survive this local texture being dropped.
    }
}

fn check_markers(camera: &Camera2D, image: &Image) -> u32 {
    let mut checked = 0;
    for (center, color) in MARKERS {
        for dy in [-3., 0., 3.] {
            for dx in [-3., 0., 3.] {
                let world = center + vec2(dx, dy);
                let physical = camera.world_to_target(world).unwrap();
                assert_eq!(
                    pixel(image, physical),
                    color,
                    "drawn marker at {world:?} -> {physical:?}"
                );
                let recovered = camera.target_to_world(physical).unwrap();
                assert!((world - recovered).length() < 0.001);
                checked += 1;
            }
        }
    }
    checked
}

pub async fn run() -> u32 {
    let target = render_target(SIZE.x as u32, SIZE.y as u32);
    target.texture.set_filter(FilterMode::Nearest);
    let mut checked = 0;
    for rotation in [0., 31., -65.] {
        for viewport in [None, Some((17, 11, 280, 145))] {
            let camera = Camera2D {
                rotation,
                viewport,
                render_target: Some(target.clone()),
                ..Camera2D::from_display_rect(Rect::new(0., 0., SIZE.x, SIZE.y))
            };
            set_camera(&camera);
            clear_background(BLACK);
            draw_markers();
            unsafe { get_internal_gl().flush() };
            checked += check_markers(&camera, &target.texture.get_texture_data());

            // Nested HUD/camera scopes must restore both an explicit viewport
            // and the default None viewport, together with the matrix/pass.
            clear_background(BLACK);
            push_camera_state();
            set_default_camera();
            clear_background(BLACK);
            push_camera_state();
            set_camera(&Camera2D {
                viewport: Some((0, 0, 40, 30)),
                ..Default::default()
            });
            pop_camera_state();
            let center = vec2(screen_width(), screen_height()) * 0.5;
            draw_rectangle(center.x - 8., center.y - 8., 16., 16., WHITE);
            assert_eq!(
                pixel(&get_screen_data(), logical_to_physical(center)),
                [255; 4],
                "nested camera pop restores the full-window HUD viewport"
            );
            checked += 1;
            pop_camera_state();
            draw_markers();
            unsafe { get_internal_gl().flush() };
            checked += check_markers(&camera, &target.texture.get_texture_data());

            for flip_x in [false, true] {
                for flip_y in [false, true] {
                    set_default_camera();
                    clear_background(BLACK);
                    let scale = ((screen_width() - 38.) / SIZE.x)
                        .min((screen_height() - 54.) / SIZE.y)
                        .max(0.1);
                    let display = Rect::new(19., 27., SIZE.x * scale, SIZE.y * scale);
                    draw_texture_ex(
                        &target.texture,
                        display.x,
                        display.y,
                        WHITE,
                        DrawTextureParams {
                            dest_size: Some(display.size()),
                            flip_x,
                            flip_y,
                            ..Default::default()
                        },
                    );
                    let image = get_screen_data();
                    for (world, color) in MARKERS {
                        let point = camera.world_to_target(world).unwrap();
                        let window =
                            render_target_to_window(point, SIZE, display, flip_x, flip_y).unwrap();
                        assert_eq!(pixel(&image, logical_to_physical(window)), color,
                            "display flip=({flip_x},{flip_y}) rotation={rotation} viewport={viewport:?}");
                        let picked = window_to_render_target(window, SIZE, display, flip_x, flip_y)
                            .and_then(|p| camera.target_to_world(p))
                            .unwrap();
                        assert!((picked - world).length() < 0.001);
                        checked += 1;
                    }
                    assert!(
                        window_to_render_target(vec2(2., 2.), SIZE, display, flip_x, flip_y)
                            .is_none()
                    );
                    next_frame().await;
                }
            }
        }
    }
    // Window conversions use physical framebuffer dimensions and viewport even
    // though the default HUD and input APIs use logical window coordinates.
    let dpi = screen_dpi_scale();
    let camera = Camera2D {
        rotation: 23.,
        viewport: Some((
            (13. * dpi) as i32,
            (17. * dpi) as i32,
            ((screen_width() - 30.) * dpi) as i32,
            ((screen_height() - 40.) * dpi) as i32,
        )),
        ..Camera2D::from_display_rect(Rect::new(0., 0., SIZE.x, SIZE.y))
    };
    set_camera(&camera);
    clear_background(BLACK);
    draw_markers();
    checked += check_markers(&camera, &get_screen_data());
    set_default_camera();
    next_frame().await;
    checked
}
