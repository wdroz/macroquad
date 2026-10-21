//! The same combined flow runs in the native and WebGL check host.
use crate::{scene, sheet};
use macroquad::experimental::animation::{SpriteRegion, SpriteRegionDrawParams};
use macroquad::prelude::*;
use macroquad::ui::{root_ui, InputHandler, KeyCode as UiKey};

async fn settle(scene: &mut scene::Scene) {
    for _ in 0..3 {
        scene.frame(0.);
        next_frame().await;
    }
}

async fn press(scene: &mut scene::Scene, key: UiKey, shift: bool) {
    root_ui().key_down(key, shift, false);
    scene.frame(0.);
    next_frame().await;
    root_ui().key_up(key);
    settle(scene).await;
}

pub async fn run() -> u32 {
    let mut scene = scene::Scene::new();
    settle(&mut scene).await;
    scene.frame(0.375);
    next_frame().await;
    assert_eq!(scene.sprite.elapsed(), 0.375);
    scene.pick(scene.sprite_window_position());
    assert_eq!(
        scene.hits, 1,
        "camera maps the displayed anchor back to the sprite"
    );
    scene.pick(vec2(0., 0.));
    assert_eq!(scene.hits, 1, "letterbox bars are not world input");

    root_ui().clear_input_focus();
    press(&mut scene, UiKey::Tab, false).await;
    assert_eq!(root_ui().focused_widget(), Some(scene::PAUSE));
    // Opening with a large elapsed time must freeze on the transition frame.
    root_ui().key_down(UiKey::Enter, false, false);
    scene.frame(9.25);
    next_frame().await;
    root_ui().key_up(UiKey::Enter);
    settle(&mut scene).await;
    assert!(scene.paused);
    assert_eq!(scene.sprite.elapsed(), 0.375);
    assert_eq!(root_ui().focused_widget(), Some(scene::RESUME));
    scene.pick(scene.sprite_window_position());
    scene.frame(20.);
    next_frame().await;
    assert_eq!(scene.hits, 1, "a modal blocks world picking");
    assert_eq!(
        scene.sprite.elapsed(),
        0.375,
        "pause retains fractional progress"
    );

    press(&mut scene, UiKey::Tab, true).await;
    assert_eq!(root_ui().focused_widget(), Some(scene::RESTART));
    press(&mut scene, UiKey::Tab, false).await;
    assert_eq!(root_ui().focused_widget(), Some(scene::RESUME));
    root_ui().key_down(UiKey::Space, false, false);
    scene.frame(9.25);
    next_frame().await;
    root_ui().key_up(UiKey::Space);
    settle(&mut scene).await;
    assert!(!scene.paused);
    assert_eq!(
        scene.sprite.elapsed(),
        0.375,
        "closing does not catch up paused time"
    );
    assert_eq!(root_ui().focused_widget(), Some(scene::PAUSE));
    scene.frame(0.125);
    next_frame().await;
    assert_eq!(
        scene.sprite.current_frame(),
        1,
        "resume continues the partial frame"
    );
    press(&mut scene, UiKey::Enter, false).await;
    press(&mut scene, UiKey::Tab, false).await;
    press(&mut scene, UiKey::Space, false).await;
    assert!(scene.paused);
    assert_eq!(scene.restarts, 1);
    assert_eq!(scene.hits, 0);
    assert_eq!(scene.sprite.elapsed(), 0.);
    press(&mut scene, UiKey::Tab, true).await;
    press(&mut scene, UiKey::Enter, false).await;
    assert!(!scene.paused);
    root_ui().clear_input_focus();

    // Queue a checked region through nested clips, then drop its LAST external
    // texture owner before submission. The marker verifies clip restoration.
    let target = render_target(80, 60);
    set_camera(&Camera2D {
        render_target: Some(target.clone()),
        ..Camera2D::from_display_rect(Rect::new(0., 0., 80., 60.))
    });
    clear_background(BLACK);
    with_clip_rect(Rect::new(24., 12., 32., 36.), || {
        with_clip_rect(Rect::new(30., 0., 40., 60.), || {
            let texture = sheet::texture();
            SpriteRegion::from_grid(sheet::SHEET_SIZE, sheet::FRAME_SIZE, 1, 0)
                .unwrap()
                .with_pivot(sheet::PIVOT)
                .unwrap()
                .draw(
                    &texture,
                    vec2(40., 30.),
                    WHITE,
                    SpriteRegionDrawParams {
                        dest_size: Some(vec2(40., 40.)),
                        ..Default::default()
                    },
                )
                .unwrap();
        });
    });
    draw_rectangle(4., 4., 4., 4., WHITE);
    // Only the GPU fixture needs an explicit flush for immediate readback.
    unsafe { get_internal_gl().flush() };
    let image = target.texture.get_texture_data();
    for y in 0..60_u32 {
        for x in 0..80_u32 {
            let mut expected = [0, 0, 0, 255];
            if (30..56).contains(&x) && (12..48).contains(&y) {
                let rgba = sheet::frame_pixel(
                    1,
                    ((x as f32 - 20.) * 0.5).floor() as i32,
                    ((y as f32 - 10.) * 0.5).floor() as i32,
                );
                if rgba[3] != 0 {
                    expected = rgba;
                }
            }
            if (4..8).contains(&x) && (4..8).contains(&y) {
                expected = [255; 4];
            }
            let index = (((59 - y) * 80 + x) * 4) as usize;
            assert_eq!(
                &image.bytes[index..index + 4],
                &expected,
                "queued clipped sprite ({x},{y})"
            );
        }
    }
    set_default_camera();
    next_frame().await;
    80 * 60
}
