//! Pick a box with the mouse.
//!
//! `Camera3D::screen_to_ray` unprojects the cursor. The ray starts at the
//! camera and uses the same view-projection as `set_camera`. The closest box
//! turns red. Escape quits.

use macroquad::prelude::*;

#[macroquad::main("Picking ray")]
async fn main() {
    let boxes = [
        Aabb::from_center_size(vec3(-1.6, 0.5, 0.0), vec3(1.1, 1.0, 1.1)),
        Aabb::from_center_size(vec3(1.5, 0.7, -0.4), vec3(1.0, 1.4, 1.0)),
        Aabb::from_center_size(vec3(0.1, 0.4, 1.6), vec3(1.3, 0.8, 0.9)),
    ];
    let camera = Camera3D {
        position: vec3(5.5, 4.5, 6.5),
        target: vec3(0.0, 0.5, 0.0),
        up: Vec3::Y,
        ..Default::default()
    };

    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }

        let (mouse_x, mouse_y) = mouse_position();
        let ray = camera.screen_to_ray(vec2(mouse_x, mouse_y));
        let closest = ray.closest_aabb(&boxes);

        clear_background(LIGHTGRAY);
        set_camera(&camera);

        draw_grid(20, 1., BLACK, GRAY);
        for (index, box_aabb) in boxes.iter().enumerate() {
            let hit = closest
                .map(|closest| closest.index == index)
                .unwrap_or(false);
            let color = if hit { RED } else { DARKGREEN };
            draw_cube_wires(box_aabb.center(), box_aabb.size(), color);
        }
        let ray_end = if let Some(closest) = closest {
            draw_sphere(closest.hit.point, 0.08, None, YELLOW);
            closest.hit.point
        } else {
            ray.point_at(30.0)
        };
        draw_line_3d(ray.origin, ray_end, YELLOW);

        set_default_camera();
        draw_text(
            "Move the mouse. The box under the cursor turns red.",
            16.0,
            28.0,
            22.0,
            BLACK,
        );
        draw_text(
            "Camera3D::screen_to_ray. Escape quits.",
            16.0,
            54.0,
            22.0,
            DARKGRAY,
        );

        next_frame().await
    }
}
