//! Filled rounded panels in the default logical-window coordinate space.
//! Run with `cargo run --example rounded_panels`. No external assets are needed.
use macroquad::prelude::*;

#[macroquad::main("Rounded panels")]
async fn main() {
    let mut radius: f32 = 16.;
    loop {
        if is_key_pressed(KeyCode::Left) {
            radius = (radius - 4.).max(0.);
        }
        if is_key_pressed(KeyCode::Right) {
            radius += 4.;
        }
        clear_background(Color::from_rgba(18, 28, 45, 255));
        for y in (0..screen_height() as usize).step_by(24) {
            for x in (0..screen_width() as usize).step_by(24) {
                if (x / 24 + y / 24) % 2 == 0 {
                    draw_rectangle(x as f32, y as f32, 24., 24., Color::new(0.2, 0.3, 0.4, 1.));
                }
            }
        }
        let width = (screen_width() - 40.).clamp(20., 520.);
        draw_rectangle_rounded(20., 80., width, 140., radius, Color::new(0.3, 0.6, 1., 0.5));
        draw_text(
            "One translucent fill: no darker internal seams",
            30.,
            155.,
            20.,
            WHITE,
        );
        draw_rectangle_rounded(20., 240., width, 90., 0., Color::new(1., 0.8, 0.3, 0.5));
        draw_text("Zero radius gives square corners", 30., 290., 20., WHITE);
        draw_rectangle_rounded(20., 350., width, 90., 1000., Color::new(0.4, 1., 0.7, 0.5));
        draw_text(
            "Oversized radius clamps to half the height",
            30.,
            400.,
            20.,
            WHITE,
        );
        draw_text(
            format!("Left / Right: radius {radius:.0}"),
            20.,
            30.,
            24.,
            WHITE,
        );
        next_frame().await;
    }
}
