//! A caller-owned AABB character: full-body sweeps, grounding and slope limits.
//! Arrows/WASD move, Space jumps, P pauses, R resets, Escape quits.

use macroquad::prelude::*;

fn initial_body() -> Aabb {
    Aabb::from_center_size(vec3(-3.0, 0.9, 3.0), vec3(0.6, 1.8, 0.6))
}

#[macroquad::main("AABB character controller")]
async fn main() {
    let boxes = [
        Aabb::from_center_size(vec3(0.0, -0.5, 0.0), vec3(16.0, 1.0, 16.0)),
        Aabb::from_center_size(vec3(0.0, 1.0, -2.0), vec3(0.02, 2.0, 4.0)),
        Aabb::from_center_size(vec3(0.0, 3.0, 1.0), vec3(3.0, 0.1, 2.0)),
    ];
    let ramps = [
        Obb::new(
            vec3(3.0, 1.0, 0.0),
            vec3(3.0, 0.1, 2.0),
            Quat::from_rotation_z(0.35),
        ),
        Obb::new(
            vec3(-4.0, 1.0, -3.0),
            vec3(2.0, 0.1, 1.5),
            Quat::from_rotation_z(1.0),
        ),
    ];
    let rocks = [Sphere::new(vec3(3.5, 0.5, 4.0), 0.5)];
    let posts = [Capsule::new(
        vec3(-4.0, 0.4, 0.0),
        vec3(-4.0, 1.6, 0.0),
        0.4,
    )];
    let solids = CharacterSolids {
        aabbs: &boxes,
        obbs: &ramps,
        spheres: &rocks,
        capsules: &posts,
    };
    // Trigger bounds intentionally stay outside the solid slices.
    let trigger = Aabb::from_center_size(vec3(1.5, 1.0, 4.0), vec3(2.0, 2.0, 1.0));
    let controller = CharacterController::default();
    let mut body = initial_body();
    let mut velocity = Vec3::ZERO;
    let mut on_ground = false;
    let mut ground_normal = None;
    let mut paused = false;

    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        if is_key_pressed(KeyCode::P) {
            paused = !paused;
        }
        if is_key_pressed(KeyCode::R) || body.center().y < -15.0 {
            body = initial_body();
            velocity = Vec3::ZERO;
            on_ground = false;
            ground_normal = None;
        }
        if !paused {
            let dt = get_frame_time().min(0.05);
            let axis = |negative, positive| {
                (is_key_down(positive) as i32 - is_key_down(negative) as i32) as f32
            };
            let wish = vec3(
                axis(KeyCode::Left, KeyCode::Right) + axis(KeyCode::A, KeyCode::D),
                0.0,
                axis(KeyCode::Up, KeyCode::Down) + axis(KeyCode::W, KeyCode::S),
            )
            .normalize_or_zero()
                * 3.5;
            velocity.x = wish.x;
            velocity.z = wish.z;
            // Rebuild grounded wish velocity before gravity: a walkable ramp's
            // returned tangent can have positive Y without requesting a jump.
            if on_ground {
                velocity.y = 0.0;
            }
            if on_ground && is_key_pressed(KeyCode::Space) {
                velocity.y = 5.5;
            }
            velocity.y -= 14.0 * dt;
            let moved = controller.move_body(body, velocity, dt, solids);
            body = moved.body;
            velocity = moved.velocity;
            on_ground = moved.on_ground;
            ground_normal = moved.ground_normal;
        }
        clear_background(LIGHTGRAY);
        set_camera(&Camera3D {
            position: vec3(13.0, 11.0, 14.0),
            target: vec3(0.0, 0.5, 0.0),
            up: Vec3::Y,
            ..Default::default()
        });
        for shape in &boxes {
            draw_cube(shape.center(), shape.size(), None, GRAY);
        }
        draw_obb(ramps[0], DARKGREEN);
        draw_obb(ramps[1], MAROON);
        for rock in &rocks {
            draw_sphere(rock.center, rock.radius, None, DARKGRAY);
        }
        for post in &posts {
            draw_capsule_wires(*post, DARKBLUE);
        }
        draw_aabb(
            trigger,
            if trigger.overlaps(&body) {
                PURPLE
            } else {
                BLUE
            },
        );
        draw_cube(
            body.center(),
            body.size(),
            None,
            if on_ground { GREEN } else { ORANGE },
        );
        if let Some(normal) = ground_normal {
            draw_line_3d(body.center(), body.center() + normal, RED);
        }
        set_default_camera();
        draw_text(
            "Arrows/WASD move | Space jump | P pause | R reset",
            16.0,
            28.0,
            22.0,
            BLACK,
        );
        draw_text(
            "Green ramp is walkable; red is too steep. Blue box is a trigger.",
            16.0,
            54.0,
            20.0,
            BLACK,
        );
        draw_text(
            format!("ground: {on_ground} | normal: {ground_normal:?} | paused: {paused}"),
            16.0,
            80.0,
            18.0,
            DARKGRAY,
        );
        next_frame().await;
    }
}
