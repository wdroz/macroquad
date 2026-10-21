//! A custom scrollable document with nested image/text clipping and a HUD.
use macroquad::prelude::*;

#[macroquad::main("Scoped document clipping")]
async fn main() {
    let image = Texture2D::from_rgba8(
        2,
        2,
        &[
            40, 140, 255, 255, 230, 150, 50, 255, 230, 150, 50, 255, 40, 140, 255, 255,
        ],
    );
    image.set_filter(FilterMode::Nearest);
    let mut scroll = 0_f32;
    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        scroll = (scroll - mouse_wheel().1 * 24.).clamp(0., 500.);
        set_default_camera();
        clear_background(Color::from_rgba(18, 28, 45, 255));
        let panel = Rect::new(
            20.,
            70.,
            (screen_width() - 40.).max(1.),
            (screen_height() - 100.).max(1.),
        );
        draw_rectangle_rounded(
            panel.x - 2.,
            panel.y - 2.,
            panel.w + 4.,
            panel.h + 4.,
            10.,
            GRAY,
        );
        with_clip_rect(panel, || {
            draw_rectangle(panel.x, panel.y, panel.w, panel.h, WHITE);
            for line in 0..30 {
                draw_text(
                    format!("Document line {}", line + 1),
                    panel.x + 12.,
                    panel.y + 30. + line as f32 * 28. - scroll,
                    22.,
                    BLACK,
                );
            }
            with_clip_rect(
                Rect::new(panel.x + panel.w - 110., panel.y + 18., 90., 90.),
                || {
                    draw_texture_ex(
                        &image,
                        panel.x + panel.w - 130.,
                        panel.y - scroll,
                        WHITE,
                        DrawTextureParams {
                            dest_size: Some(vec2(180., 180.)),
                            ..Default::default()
                        },
                    );
                },
            );
            // A disjoint nested clip draws nothing and cannot reopen its parent.
            with_clip_rect(Rect::new(-200., -200., 10., 10.), || {
                draw_circle(40., 40., 1000., RED)
            });
        });
        draw_text("Wheel: scroll | Esc: quit", 20., 35., 24., WHITE);
        draw_text(
            "HUD restored outside the document",
            20.,
            screen_height() - 10.,
            18.,
            YELLOW,
        );
        next_frame().await;
    }
}
