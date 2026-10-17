//! Yaw an oriented box into a static AABB and a sphere.
//!
//! `Obb::from_world_matrix` uses the same full size as `draw_cube`. The wire
//! box is this example's own outline; `draw_obb` is a later roadmap item.

use macroquad::prelude::*;

fn draw_obb(obb: Obb, color: Color) {
    let rotation = obb.rotation.normalize();
    let half = obb.half_extents;
    let corner =
        |x: f32, y: f32, z: f32| obb.center + rotation * vec3(x * half.x, y * half.y, z * half.z);
    let corners = [
        corner(-1.0, -1.0, -1.0),
        corner(1.0, -1.0, -1.0),
        corner(1.0, 1.0, -1.0),
        corner(-1.0, 1.0, -1.0),
        corner(-1.0, -1.0, 1.0),
        corner(1.0, -1.0, 1.0),
        corner(1.0, 1.0, 1.0),
        corner(-1.0, 1.0, 1.0),
    ];
    let edges = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    for (start, end) in edges {
        draw_line_3d(corners[start], corners[end], color);
    }
}

#[macroquad::main("OBB overlap")]
async fn main() {
    let static_box = Aabb::from_center_size(vec3(1.6, 0.4, 0.0), vec3(1.0, 0.8, 1.0));
    let orb = Sphere::new(vec3(0.0, 0.4, 1.5), 0.35);
    let mut center = vec3(0.0, 0.4, 0.0);
    let ray_origin = vec3(0.0, 0.4, 4.0);
    let ray_direction = vec3(0.0, 0.0, -1.0);

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
        if step.length_squared() > 0.0 {
            center += step.normalize() * 2.2 * dt;
        }

        let yaw = Quat::from_rotation_y(get_time() as f32 * 0.7);
        let player = Obb::from_world_matrix(Mat4::from_scale_rotation_translation(
            vec3(2.6, 0.6, 0.6),
            yaw,
            center,
        ));
        let hits_box = player.overlaps_aabb(&static_box);
        let hits_sphere = player.overlaps_sphere(&orb);
        let ray_hit = player.ray_intersect(ray_origin, ray_direction);

        clear_background(LIGHTGRAY);

        set_camera(&Camera3D {
            position: vec3(6.0, 5.0, 6.0),
            up: vec3(0.0, 1.0, 0.0),
            target: vec3(0.4, 0.4, 0.2),
            ..Default::default()
        });

        draw_grid(20, 1., BLACK, GRAY);
        draw_cube_wires(
            static_box.center(),
            static_box.size(),
            if hits_box { RED } else { DARKGREEN },
        );
        draw_sphere(
            orb.center,
            orb.radius,
            None,
            if hits_sphere { RED } else { GOLD },
        );
        draw_obb(
            player,
            if hits_box || hits_sphere {
                ORANGE
            } else {
                BLUE
            },
        );

        let ray_end = if let Some(hit) = ray_hit {
            draw_sphere(hit.point, 0.08, None, YELLOW);
            if hit.normal != Vec3::ZERO {
                draw_line_3d(hit.point, hit.point + hit.normal * 0.6, ORANGE);
            }
            hit.point
        } else {
            ray_origin + ray_direction * 8.0
        };
        draw_line_3d(ray_origin, ray_end, YELLOW);

        set_default_camera();
        draw_text(
            "Arrows move the box. It yaws on its own and turns orange on a hit.",
            16.0,
            28.0,
            22.0,
            BLACK,
        );
        draw_text(
            "Yellow ray: Obb::ray_intersect. Full size matches draw_cube.",
            16.0,
            54.0,
            22.0,
            DARKGRAY,
        );

        next_frame().await
    }
}
