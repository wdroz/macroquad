//! A fixed elapsed-time script selects source regions; every target pixel is checked.
use crate::sheet;
use macroquad::experimental::animation::{
    AnimatedSprite, Animation, AnimationPlayback, SpriteRegion, SpriteRegionDrawParams,
};
use macroquad::prelude::*;

async fn check(
    sprite: &AnimatedSprite,
    frame: u32,
    finished: bool,
    flip: bool,
    camera: &Camera2D,
    target: &RenderTarget,
    texture: &Texture2D,
) -> u32 {
    assert_eq!(sprite.current_frame(), frame);
    assert_eq!(sprite.is_finished(), finished);
    set_camera(camera);
    clear_background(BLACK);
    SpriteRegion::new(sheet::SHEET_SIZE, sprite.frame().source_rect)
        .unwrap()
        .with_pivot(sheet::PIVOT)
        .unwrap()
        .draw(
            texture,
            vec2(40., 30.),
            WHITE,
            SpriteRegionDrawParams {
                dest_size: Some(vec2(40., 40.)),
                flip_x: flip,
                ..Default::default()
            },
        )
        .unwrap();
    unsafe { get_internal_gl().flush() };
    let image = target.texture.get_texture_data();
    for y in 0..60_u32 {
        for x in 0..80_u32 {
            let sx = (x as f32 + 0.5 - 40.) / 2. * if flip { -1. } else { 1. } + 10.;
            let sy = (y as f32 + 0.5 - 30.) / 2. + 10.;
            let mut expected =
                sheet::frame_pixel(frame as usize, sx.floor() as i32, sy.floor() as i32);
            if expected[3] == 0 {
                expected = [0, 0, 0, 255];
            }
            let i = (((59 - y) * 80 + x) * 4) as usize;
            assert_eq!(
                &image.bytes[i..i + 4],
                &expected,
                "frame={frame}, flip={flip}, ({x},{y})"
            );
        }
    }
    next_frame().await;
    80 * 60
}

pub async fn run() -> u32 {
    let target = render_target(80, 60);
    let camera = Camera2D {
        render_target: Some(target.clone()),
        ..Camera2D::from_display_rect(Rect::new(0., 0., 80., 60.))
    };
    let texture = sheet::texture();
    let mut sprite = AnimatedSprite::try_new(
        20,
        20,
        &[Animation {
            name: "pulse".into(),
            row: 0,
            frames: 2,
            fps: 2,
        }],
        true,
    )
    .unwrap();
    let mut count = 0;
    macro_rules! verify {
        ($frame:expr, $finished:expr) => {
            count += check(
                &sprite,
                $frame,
                $finished,
                count % 9600 != 0,
                &camera,
                &target,
                &texture,
            )
            .await;
        };
    }
    verify!(0, false);
    sprite.update_with_delta(0.25).unwrap();
    verify!(0, false);
    sprite.update_with_delta(0.25).unwrap();
    verify!(1, false);
    sprite.update_with_delta(0.125).unwrap();
    verify!(1, false);
    sprite.update_with_delta(2.5).unwrap();
    verify!(0, false);
    sprite.update_with_delta(0.375).unwrap();
    verify!(1, false);
    sprite.playing = false;
    sprite.update_with_delta(100.).unwrap();
    verify!(1, false);
    sprite.playing = true;
    sprite.update_with_delta(0.5).unwrap();
    verify!(0, false);
    sprite.seek(0.5).unwrap();
    verify!(1, false);
    sprite.reset();
    verify!(0, false);
    sprite.set_playback(AnimationPlayback::Once);
    sprite.seek(0.75).unwrap();
    verify!(1, false);
    sprite.update_with_delta(0.25).unwrap();
    verify!(1, true);
    sprite.update_with_delta(1e200).unwrap();
    verify!(1, true);
    sprite.seek(0.25).unwrap();
    verify!(0, false);
    sprite.update_with_delta(0.25).unwrap();
    verify!(1, false);
    set_default_camera();
    next_frame().await;
    count
}
