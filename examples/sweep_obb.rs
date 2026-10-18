//! Move a sphere into a yawed cover. `Sphere::sweep_obb` stops it on the face.
//!
//! Arrow keys move on the XZ plane. The cover is an `Obb` with a thin axis,
//! yawed about Y. A step that would pass through the box still reports
//! `SweepHit`. The leftover motion stays on the face, so holding into the
//! cover slides along it. A sphere that only touches the cover is not an
//! impact.

use macroquad::prelude::*;

fn move_sphere(ball: Sphere, delta: Vec3, cover: &Obb) -> (Sphere, Option<SweepHit>) {
    let Some(hit) = ball.sweep_obb(delta, cover) else {
        return (ball.offset(delta), None);
    };
    let mut rest = delta * (1.0 - hit.t);
    let into = rest.dot(hit.normal);
    if into < 0.0 {
        rest -= hit.normal * into;
    }
    (ball.offset(delta * hit.t + rest), Some(hit))
}

#[macroquad::main("Sweep OBB")]
async fn main() {
    let yaw = Quat::from_rotation_y(0.5);
    let cover = Obb::new(vec3(0.4, 0.6, 0.0), vec3(1.2, 0.6, 0.225), yaw);
    let mut ball = Sphere::new(vec3(-1.5, 0.5, 0.35), 0.12);

    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }

        let dt = get_frame_time();
        let mut step = Vec3::ZERO;
        if is_key_down(KeyCode::Left) {
            step.x -= 1.0;
        }
        if is_key_down(KeyCode::Right) {
            step.x += 1.0;
        }
        if is_key_down(KeyCode::Up) {
            step.z -= 1.0;
        }
        if is_key_down(KeyCode::Down) {
            step.z += 1.0;
        }
        let delta = if step.length_squared() > 0.0 {
            step.normalize() * dt * 2.2
        } else {
            Vec3::ZERO
        };

        let (moved, hit) = move_sphere(ball, delta, &cover);
        ball = moved;

        clear_background(LIGHTGRAY);
        set_camera(&Camera3D {
            position: vec3(4.2, 3.6, 5.4),
            up: vec3(0.0, 1.0, 0.0),
            target: vec3(0.2, 0.4, 0.0),
            ..Default::default()
        });

        draw_grid(20, 1., BLACK, GRAY);
        draw_obb(cover, DARKGREEN);
        let ball_color = if hit.is_some() { RED } else { ORANGE };
        draw_sphere_wires(ball.center, ball.radius, None, ball_color);
        if let Some(hit) = hit {
            draw_line_3d(hit.point, hit.point + hit.normal * 0.6, YELLOW);
            draw_sphere(hit.point, 0.05, None, YELLOW);
        }

        set_default_camera();
        draw_text(
            "Arrows move the sphere. Red means Sphere::sweep_obb hit the cover.",
            16.0,
            28.0,
            22.0,
            BLACK,
        );
        draw_text(
            "The cover is a yawed Obb. A touch does not block a sideways step.",
            16.0,
            54.0,
            22.0,
            DARKGRAY,
        );

        next_frame().await
    }
}
