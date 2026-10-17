//! Yaw a ray with the arrow keys and mark the closest box or sphere.
//!
//! `Ray::closest_aabb` and `Ray::closest_sphere` reject hits behind the
//! origin (`t < 0`). The two hits are compared by `t` because they share a
//! direction.

use macroquad::prelude::*;

#[macroquad::main("Ray cast")]
async fn main() {
    let boxes = [
        Aabb::from_center_size(vec3(-2.0, 0.5, 0.0), vec3(1.2, 1.0, 1.2)),
        Aabb::from_center_size(vec3(2.0, 0.6, -0.4), vec3(1.0, 1.2, 1.0)),
    ];
    let spheres = [
        Sphere::new(vec3(0.0, 0.55, -1.6), 0.55),
        Sphere::new(vec3(-0.5, 0.4, 1.4), 0.4),
    ];
    let origin = vec3(0.0, 0.6, 4.5);
    let mut yaw = 0.0_f32;

    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }

        let dt = get_frame_time();
        if is_key_down(KeyCode::Left) {
            yaw -= dt;
        }
        if is_key_down(KeyCode::Right) {
            yaw += dt;
        }

        let ray = Ray::new(origin, vec3(yaw.sin(), 0.0, -yaw.cos())).normalized();
        let box_hit = ray.closest_aabb(&boxes);
        let sphere_hit = ray.closest_sphere(&spheres);
        let box_wins = match (box_hit, sphere_hit) {
            (Some(boxes_closest), Some(spheres_closest)) => {
                boxes_closest.hit.t <= spheres_closest.hit.t
            }
            (Some(_), None) => true,
            _ => false,
        };
        let closest = match (box_hit, sphere_hit) {
            (Some(boxes_closest), Some(_)) if box_wins => Some(boxes_closest.hit),
            (Some(_), Some(spheres_closest)) => Some(spheres_closest.hit),
            (Some(boxes_closest), None) => Some(boxes_closest.hit),
            (None, Some(spheres_closest)) => Some(spheres_closest.hit),
            (None, None) => None,
        };

        clear_background(LIGHTGRAY);

        set_camera(&Camera3D {
            position: vec3(8.0, 8.0, 8.0),
            up: vec3(0.0, 1.0, 0.0),
            target: vec3(0.0, 0.5, 0.0),
            ..Default::default()
        });

        draw_grid(20, 1., BLACK, GRAY);
        for (index, box_aabb) in boxes.iter().enumerate() {
            let hit = box_wins
                && box_hit
                    .map(|closest| closest.index == index)
                    .unwrap_or(false);
            let color = if hit { RED } else { DARKGREEN };
            draw_cube_wires(box_aabb.center(), box_aabb.size(), color);
        }
        for (index, sphere) in spheres.iter().enumerate() {
            let hit = !box_wins
                && sphere_hit
                    .map(|closest| closest.index == index)
                    .unwrap_or(false);
            let color = if hit { RED } else { BLUE };
            draw_sphere(sphere.center, sphere.radius, None, color);
        }

        let ray_end = if let Some(hit) = closest {
            draw_sphere(hit.point, 0.12, None, YELLOW);
            if hit.normal != Vec3::ZERO {
                draw_line_3d(hit.point, hit.point + hit.normal * 0.75, ORANGE);
            }
            hit.point
        } else {
            ray.point_at(12.0)
        };
        draw_line_3d(ray.origin, ray_end, YELLOW);

        set_default_camera();
        draw_text(
            "Left and Right yaw the ray. The closest shape turns red.",
            16.0,
            28.0,
            22.0,
            BLACK,
        );
        draw_text(
            "Ray::closest_aabb and Ray::closest_sphere. Hits with t < 0 are ignored.",
            16.0,
            54.0,
            22.0,
            DARKGRAY,
        );

        next_frame().await
    }
}
