//! Original 40 x 20 sheet generated in memory; no external asset is required.
use macroquad::experimental::animation::SpriteRegion;
use macroquad::prelude::*;

pub const SHEET_SIZE: Vec2 = Vec2::new(40., 20.);
pub const FRAME_SIZE: Vec2 = Vec2::new(20., 20.);
pub const PIVOT: Vec2 = Vec2::new(10., 10.);

pub fn frame_pixel(frame: usize, x: i32, y: i32) -> [u8; 4] {
    let (left, top, right, bottom) = if frame == 0 {
        (2, 6, 18, 16)
    } else {
        (5, 2, 15, 16)
    };
    if x < left || x >= right || y < top || y >= bottom {
        return [0, 0, 0, 0];
    }
    if (8..12).contains(&x) && (8..12).contains(&y) {
        return [255, 0, 255, 255];
    }
    if (6..8).contains(&x) && (6..8).contains(&y) {
        return [255, 0, 0, 255];
    }
    if (12..14).contains(&x) && (12..14).contains(&y) {
        return [0, 255, 0, 255];
    }
    if frame == 0 {
        [240, 160, 40, 255]
    } else {
        [40, 180, 220, 255]
    }
}

pub fn texture() -> Texture2D {
    let mut pixels = Vec::with_capacity(40 * 20 * 4);
    for y in 0..20 {
        for x in 0..40 {
            pixels.extend_from_slice(&frame_pixel((x / 20) as usize, x % 20, y));
        }
    }
    let texture = Texture2D::from_rgba8(40, 20, &pixels);
    texture.set_filter(FilterMode::Nearest);
    texture
}

pub fn region(frame: usize, trimmed: bool) -> SpriteRegion {
    let region = if trimmed {
        let (source, offset) = if frame == 0 {
            (Rect::new(2., 6., 16., 10.), vec2(2., 6.))
        } else {
            (Rect::new(25., 2., 10., 14.), vec2(5., 2.))
        };
        SpriteRegion::trimmed(SHEET_SIZE, source, FRAME_SIZE, offset)
    } else {
        SpriteRegion::from_grid(SHEET_SIZE, FRAME_SIZE, frame as u32, 0)
    };
    region.unwrap().with_pivot(PIVOT).unwrap()
}
