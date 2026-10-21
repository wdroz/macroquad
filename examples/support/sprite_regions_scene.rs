use crate::sheet;
use macroquad::experimental::animation::SpriteRegionDrawParams;
use macroquad::prelude::*;

pub struct Scene {
    texture: Texture2D,
    pub frame: usize,
    pub flip_x: bool,
    pub flip_y: bool,
    pub turns: u32,
}
impl Scene {
    pub fn new() -> Self {
        Self {
            texture: sheet::texture(),
            frame: 0,
            flip_x: false,
            flip_y: false,
            turns: 0,
        }
    }
    pub fn anchor(&self) -> Vec2 {
        vec2(screen_width() * 0.5, screen_height() * 0.55)
    }
    pub fn draw(&mut self) {
        if is_key_pressed(KeyCode::Space) {
            self.frame ^= 1;
        }
        if is_key_pressed(KeyCode::X) {
            self.flip_x = !self.flip_x;
        }
        if is_key_pressed(KeyCode::Y) {
            self.flip_y = !self.flip_y;
        }
        if is_key_pressed(KeyCode::R) {
            self.turns = (self.turns + 1) % 4;
        }
        set_default_camera();
        clear_background(Color::from_rgba(19, 25, 37, 255));
        let anchor = self.anchor();
        let size = (screen_width().min(screen_height()) * 0.36).clamp(80., 200.);
        draw_text("Checked sprite regions", 16., 34., 25., WHITE);
        draw_text(
            "Space: frame    X / Y: flip    R: rotate",
            16.,
            62.,
            18.,
            LIGHTGRAY,
        );
        draw_text("Magenta stays on the same anchor", 16., 85., 18., LIGHTGRAY);
        draw_line(
            anchor.x - size,
            anchor.y,
            anchor.x + size,
            anchor.y,
            1.,
            GRAY,
        );
        draw_line(
            anchor.x,
            anchor.y - size,
            anchor.x,
            anchor.y + size,
            1.,
            GRAY,
        );
        sheet::region(self.frame, true)
            .draw(
                &self.texture,
                anchor,
                WHITE,
                SpriteRegionDrawParams {
                    dest_size: Some(vec2(size, size)),
                    rotation: self.turns as f32 * std::f32::consts::FRAC_PI_2,
                    flip_x: self.flip_x,
                    flip_y: self.flip_y,
                },
            )
            .unwrap();
        draw_text(
            format!(
                "Frame {}   flip ({}, {})   {} degrees",
                self.frame + 1,
                self.flip_x,
                self.flip_y,
                self.turns * 90
            ),
            16.,
            screen_height() - 22.,
            18.,
            WHITE,
        );
    }
}
