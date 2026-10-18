//! Move a box and a sphere into a wall. The sweep stops them on the face.
//!
//! Arrow keys move on the XZ plane. `Aabb::sweep` and `Sphere::sweep_aabb`
//! return the first impact along that step. The leftover motion is kept when
//! it lies on the wall, so holding into the wall slides along it. A shape that
//! only touches the wall is not an impact, and a sideways step is free.

use macroquad::prelude::*;

fn move_box(body: Aabb, delta: Vec3, wall: &Aabb) -> (Aabb, Option<SweepHit>) {
    let Some(hit) = body.sweep(delta, wall) else {
        return (body.offset(delta), None);
    };
    let mut rest = delta * (1.0 - hit.t);
    let into = rest.dot(hit.normal);
    if into < 0.0 {
        rest -= hit.normal * into;
    }
    (body.offset(delta * hit.t + rest), Some(hit))
}

fn move_sphere(ball: Sphere, delta: Vec3, wall: &Aabb) -> (Sphere, Option<SweepHit>) {
    let Some(hit) = ball.sweep_aabb(delta, wall) else {
        return (ball.offset(delta), None);
    };
    let mut rest = delta * (1.0 - hit.t);
    let into = rest.dot(hit.normal);
    if into < 0.0 {
        rest -= hit.normal * into;
    }
    (ball.offset(delta * hit.t + rest), Some(hit))
}

#[macroquad::main("Sweep")]
async fn main() {
    let wall = Aabb::from_center_size(vec3(1.4, 0.75, 0.0), vec3(0.5, 1.5, 2.4));
    let box_size = vec3(0.7, 0.8, 0.7);
    let mut box_center = vec3(-1.3, 0.4, 0.7);
    let mut ball = Sphere::new(vec3(-1.3, 0.45, -0.7), 0.45);

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

        let body = Aabb::from_center_size(box_center, box_size);
        let (moved_box, box_hit) = move_box(body, delta, &wall);
        box_center = moved_box.center();
        let (moved_ball, ball_hit) = move_sphere(ball, delta, &wall);
        ball = moved_ball;

        clear_background(LIGHTGRAY);
        set_camera(&Camera3D {
            position: vec3(4.5, 4.2, 6.0),
            up: vec3(0.0, 1.0, 0.0),
            target: vec3(0.2, 0.4, 0.0),
            ..Default::default()
        });

        draw_grid(20, 1., BLACK, GRAY);
        draw_cube_wires(wall.center(), wall.size(), DARKGREEN);
        let box_color = if box_hit.is_some() { RED } else { BLUE };
        draw_cube_wires(box_center, box_size, box_color);
        let ball_color = if ball_hit.is_some() { RED } else { ORANGE };
        draw_sphere_wires(ball.center, ball.radius, None, ball_color);
        for hit in [box_hit, ball_hit].into_iter().flatten() {
            draw_line_3d(hit.point, hit.point + hit.normal * 0.6, YELLOW);
            draw_sphere(hit.point, 0.06, None, YELLOW);
        }

        set_default_camera();
        draw_text(
            "Arrows move the box and the sphere. Red means the sweep hit the wall.",
            16.0,
            28.0,
            22.0,
            BLACK,
        );
        draw_text(
            "Aabb::sweep and Sphere::sweep_aabb. A touch does not block a sideways step.",
            16.0,
            54.0,
            22.0,
            DARKGRAY,
        );

        next_frame().await
    }
}
