//! Move a capsule with the arrow keys and watch it overlap a box and a sphere.
//!
//! `a` and `b` are the inner segment. The rounded ends extend `radius` past
//! them. The outline is `draw_capsule_wires`.

use macroquad::prelude::*;

#[macroquad::main("Capsule overlap")]
async fn main() {
    let static_box = Aabb::from_center_size(vec3(-1.6, 0.4, 0.0), vec3(1.2, 0.8, 1.2));
    let orb = Sphere::new(vec3(1.5, 0.45, 0.8), 0.4);
    let mut body = Capsule::new(vec3(0.0, 0.45, 2.2), vec3(0.0, 1.35, 2.2), 0.35);
    let ray_origin = vec3(0.0, 0.8, 5.0);
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
            body = body.offset(step.normalize() * 2.4 * dt);
        }

        let hits_box = body.overlaps_aabb(&static_box);
        let hits_sphere = body.overlaps_sphere(&orb);
        let overlapping = hits_box || hits_sphere;
        let ray_hit = body.ray_intersect(ray_origin, ray_direction);

        clear_background(LIGHTGRAY);

        set_camera(&Camera3D {
            position: vec3(6.0, 5.0, 6.0),
            up: vec3(0.0, 1.0, 0.0),
            target: vec3(0.0, 0.6, 0.4),
            ..Default::default()
        });

        draw_grid(20, 1., BLACK, GRAY);
        draw_cube_wires(
            static_box.center(),
            static_box.size(),
            if hits_box { RED } else { DARKGREEN },
        );
        draw_sphere_wires(
            orb.center,
            orb.radius,
            None,
            if hits_sphere { RED } else { DARKGREEN },
        );
        draw_capsule_wires(body, if overlapping { ORANGE } else { BLUE });

        let ray_end = if let Some(hit) = ray_hit {
            draw_sphere(hit.point, 0.08, None, YELLOW);
            if hit.normal != Vec3::ZERO {
                draw_line_3d(hit.point, hit.point + hit.normal * 0.6, ORANGE);
            }
            hit.point
        } else {
            ray_origin + ray_direction * 10.0
        };
        draw_line_3d(ray_origin, ray_end, YELLOW);

        set_default_camera();
        draw_text(
            "Arrows move the blue capsule. It turns orange on a box or sphere.",
            16.0,
            28.0,
            22.0,
            BLACK,
        );
        draw_text(
            "Yellow ray: Capsule::ray_intersect. a and b are the inner segment.",
            16.0,
            54.0,
            22.0,
            DARKGRAY,
        );

        next_frame().await
    }
}
