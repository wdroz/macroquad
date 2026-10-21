//! The shared, asset-free scene used by the picking demo and its input fixture.
use macroquad::camera::{render_target_to_window, window_to_render_target};
use macroquad::prelude::*;

pub const TARGET_SIZE: Vec2 = Vec2::new(320., 180.);
pub const SPRITE: Rect = Rect::new(196., 52., 28., 20.);

pub fn letterbox() -> Rect {
    let scale = ((screen_width() - 32.).max(1.) / TARGET_SIZE.x)
        .min((screen_height() - 148.).max(1.) / TARGET_SIZE.y);
    let size = TARGET_SIZE * scale;
    Rect::new(
        (screen_width() - size.x) * 0.5,
        92. + (screen_height() - 148. - size.y).max(0.) * 0.5,
        size.x,
        size.y,
    )
}

pub struct Scene {
    pub camera: Camera2D,
    pub texture: Texture2D,
    pub flip_x: bool,
    pub flip_y: bool,
    pub hits: u32,
    pub misses: u32,
}

impl Scene {
    pub fn new() -> Self {
        let target = render_target(TARGET_SIZE.x as u32, TARGET_SIZE.y as u32);
        target.texture.set_filter(FilterMode::Nearest);
        // A deliberately asymmetric sprite makes accidental Y inversion visible.
        let mut image = Image::gen_image_color(14, 10, Color::from_rgba(0, 220, 240, 255));
        for y in 1..4 {
            for x in 10..13 {
                image.set_pixel(x, y, WHITE);
            }
        }
        for y in 6..10 {
            for x in 0..4 {
                image.set_pixel(x, y, ORANGE);
            }
        }
        let texture = Texture2D::from_image(&image);
        texture.set_filter(FilterMode::Nearest);
        Self {
            camera: Camera2D {
                rotation: 27.,
                viewport: Some((17, 11, 280, 145)),
                render_target: Some(target),
                ..Camera2D::from_display_rect(Rect::new(0., 0., 320., 180.))
            },
            texture,
            flip_x: false,
            flip_y: true,
            hits: 0,
            misses: 0,
        }
    }

    pub fn sprite_window_position(&self) -> Vec2 {
        let target_point = self.camera.world_to_target(SPRITE.center()).unwrap();
        render_target_to_window(
            target_point,
            TARGET_SIZE,
            letterbox(),
            self.flip_x,
            self.flip_y,
        )
        .unwrap()
    }

    pub fn frame(&mut self) {
        if is_key_pressed(KeyCode::Space) {
            self.flip_y = !self.flip_y;
        }
        if is_key_pressed(KeyCode::X) {
            self.flip_x = !self.flip_x;
        }
        if is_key_pressed(KeyCode::R) {
            self.camera.rotation = if self.camera.rotation > 0. { -37. } else { 27. };
        }
        let display = letterbox();
        // Logical input -> displayed texture -> physical target -> camera world.
        // The display flags are exactly those supplied to draw_texture_ex below.
        let pointer = window_to_render_target(
            mouse_position_logical(),
            TARGET_SIZE,
            display,
            self.flip_x,
            self.flip_y,
        )
        .and_then(|point| self.camera.target_to_world(point));
        let hovering = pointer.is_some_and(|point| SPRITE.contains(point));
        if is_mouse_button_pressed(MouseButton::Left) {
            if hovering {
                self.hits += 1;
            } else {
                self.misses += 1;
            }
        }

        set_camera(&self.camera);
        clear_background(Color::from_rgba(31, 46, 63, 255));
        draw_rectangle(0., 0., 320., 180., Color::from_rgba(40, 63, 80, 255));
        draw_line(160., 0., 160., 180., 1., GRAY);
        draw_line(0., 90., 320., 90., 1., GRAY);
        draw_circle(100., 120., 12., Color::from_rgba(240, 70, 100, 255));
        draw_texture_ex(
            &self.texture,
            SPRITE.x,
            SPRITE.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(SPRITE.size()),
                ..Default::default()
            },
        );
        if hovering {
            draw_rectangle_lines(
                SPRITE.x - 3.,
                SPRITE.y - 3.,
                SPRITE.w + 6.,
                SPRITE.h + 6.,
                1.,
                YELLOW,
            );
        }

        set_default_camera();
        clear_background(Color::from_rgba(15, 23, 35, 255));
        draw_texture_ex(
            &self.camera.render_target.as_ref().unwrap().texture,
            display.x,
            display.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(display.size()),
                flip_x: self.flip_x,
                flip_y: self.flip_y,
                ..Default::default()
            },
        );
        draw_text("Click the cyan sprite", 16., 30., 25., WHITE);
        draw_text(
            "Space: flip Y   X: flip X   R: rotate",
            16.,
            54.,
            18.,
            LIGHTGRAY,
        );
        draw_text(
            "Resize: picking follows the letterbox",
            16.,
            77.,
            18.,
            LIGHTGRAY,
        );
        draw_text(
            format!("Hits: {}   Misses: {}", self.hits, self.misses),
            16.,
            screen_height() - 25.,
            22.,
            if hovering { YELLOW } else { WHITE },
        );
    }
}
