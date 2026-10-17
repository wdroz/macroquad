//! Move an AABB with the arrow keys and watch it overlap static boxes.
//!
//! A ray along -Z reports the closest slab hit. Wireframes use the same
//! center and size as `Aabb::from_center_size`, via `draw_cube_wires`.

use macroquad::prelude::*;

fn draw_aabb(aabb: Aabb, color: Color) {
    draw_cube_wires(aabb.center(), aabb.size(), color);
}

#[macroquad::main("AABB overlap")]
async fn main() {
    let static_boxes = [
        Aabb::from_center_size(vec3(-2.0, 0.5, 0.0), vec3(1.5, 1.0, 1.5)),
        Aabb::from_center_size(vec3(2.2, 0.75, 1.0), vec3(1.2, 1.5, 1.2)),
        Aabb::from_center_size(vec3(0.0, 0.4, -2.5), vec3(3.0, 0.8, 0.8)),
    ];
    let mut player = Aabb::from_center_size(vec3(0.0, 0.5, 2.0), vec3(1.0, 1.0, 1.0));
    let ray_origin = vec3(0.0, 0.5, 6.0);
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
            player = player.offset(step.normalize() * 3.0 * dt);
        }

        let overlapping = static_boxes
            .iter()
            .any(|box_aabb| player.overlaps(box_aabb));

        let mut closest: Option<RayHit> = None;
        for target in [static_boxes[0], static_boxes[1], static_boxes[2], player] {
            if let Some(hit) = target.ray_intersect(ray_origin, ray_direction) {
                if closest.map(|best| hit.t < best.t).unwrap_or(true) {
                    closest = Some(hit);
                }
            }
        }

        clear_background(LIGHTGRAY);

        set_camera(&Camera3D {
            position: vec3(8.0, 8.0, 8.0),
            up: vec3(0.0, 1.0, 0.0),
            target: vec3(0.0, 0.5, 0.0),
            ..Default::default()
        });

        draw_grid(20, 1., BLACK, GRAY);
        for box_aabb in &static_boxes {
            let color = if player.overlaps(box_aabb) {
                RED
            } else {
                DARKGREEN
            };
            draw_aabb(*box_aabb, color);
        }
        draw_aabb(player, if overlapping { ORANGE } else { BLUE });

        let ray_end = if let Some(hit) = closest {
            draw_sphere(hit.point, 0.12, None, YELLOW);
            if hit.normal != Vec3::ZERO {
                draw_line_3d(hit.point, hit.point + hit.normal * 0.75, ORANGE);
            }
            hit.point
        } else {
            ray_origin + ray_direction * 12.0
        };
        draw_line_3d(ray_origin, ray_end, YELLOW);

        set_default_camera();
        draw_text(
            "Arrows move the blue box. It turns orange when it overlaps.",
            16.0,
            28.0,
            22.0,
            BLACK,
        );
        draw_text(
            "Yellow ray: Aabb::ray_intersect. The sphere is the closest hit.",
            16.0,
            54.0,
            22.0,
            DARKGRAY,
        );

        next_frame().await
    }
}
