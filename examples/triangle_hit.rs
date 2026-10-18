//! Yaw a ray and mark the closest triangle.
//!
//! `Ray::closest_triangle` uses Möller–Trumbore and keeps both winding
//! orders. Edges are `draw_triangle_wires`; the hit normal is the triangle's
//! winding normal.

use macroquad::prelude::*;

#[macroquad::main("Triangle hit")]
async fn main() {
    let triangles = [
        Triangle::new(
            vec3(-1.8, 0.0, 0.2),
            vec3(-0.2, 0.0, 0.2),
            vec3(-1.0, 1.5, 0.2),
        ),
        Triangle::new(
            vec3(0.3, 0.0, -1.4),
            vec3(1.8, 0.0, -1.0),
            vec3(0.8, 1.4, -1.6),
        ),
    ];
    let origin = vec3(0.0, 0.7, 3.2);
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
        let closest = ray.closest_triangle(&triangles);

        clear_background(LIGHTGRAY);

        set_camera(&Camera3D {
            position: vec3(6.0, 5.0, 6.0),
            up: vec3(0.0, 1.0, 0.0),
            target: vec3(0.0, 0.6, 0.0),
            ..Default::default()
        });

        draw_grid(20, 1., BLACK, GRAY);
        for (index, triangle) in triangles.iter().enumerate() {
            let hit = closest
                .map(|closest| closest.index == index)
                .unwrap_or(false);
            draw_triangle_wires(*triangle, if hit { RED } else { DARKGREEN });
        }

        let ray_end = if let Some(closest) = closest {
            let hit = closest.hit;
            draw_sphere(hit.point, 0.08, None, YELLOW);
            draw_line_3d(hit.point, hit.point + hit.normal * 0.75, ORANGE);
            hit.point
        } else {
            ray.point_at(8.0)
        };
        draw_line_3d(ray.origin, ray_end, YELLOW);

        set_default_camera();
        draw_text(
            "Left and Right yaw the ray. The closest triangle turns red.",
            16.0,
            28.0,
            22.0,
            BLACK,
        );
        draw_text(
            "Ray::closest_triangle. Both winding orders count.",
            16.0,
            54.0,
            22.0,
            DARKGRAY,
        );

        next_frame().await
    }
}
