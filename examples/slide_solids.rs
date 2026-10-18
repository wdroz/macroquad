//! Slide a sphere past a rock and a column.
//!
//! Arrow keys move on the XZ plane. `slide_sphere_solids` stops the sphere
//! on a floor, a `Sphere`, and a vertical `Capsule`, and keeps the motion
//! along the surface. A step that only touches a solid is not shortened.
//! The sphere turns red when the step was shortened.

use macroquad::prelude::*;

#[macroquad::main("Slide solids")]
async fn main() {
    let floor = Aabb::from_center_size(vec3(0.0, -0.5, 0.0), vec3(16.0, 1.0, 16.0));
    let rock = Sphere::new(vec3(1.6, 0.55, 0.4), 0.55);
    let column = Capsule::new(vec3(-1.4, 0.45, -1.2), vec3(-1.4, 1.7, -1.2), 0.4);
    let radius = 0.4;
    let mut center = vec3(0.0, radius, 2.2);

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
            step.normalize() * dt * 2.4 + vec3(0.0, -2.0, 0.0) * dt
        } else {
            vec3(0.0, -2.0, 0.0) * dt
        };
        let free = center + delta;
        center = slide_sphere_solids(center, radius, delta, &[floor], &[], &[rock], &[column]);
        let shortened = (center - free).length_squared() > 1e-8;

        clear_background(LIGHTGRAY);
        set_camera(&Camera3D {
            position: vec3(5.5, 4.8, 6.2),
            up: vec3(0.0, 1.0, 0.0),
            target: vec3(0.0, 0.4, 0.0),
            ..Default::default()
        });

        draw_grid(20, 1., BLACK, GRAY);
        draw_cube(
            floor.center(),
            floor.size(),
            None,
            Color::new(0.55, 0.58, 0.52, 1.0),
        );
        draw_sphere(rock.center, rock.radius, None, DARKGRAY);
        draw_capsule_wires(column, DARKGREEN);
        let color = if shortened { RED } else { ORANGE };
        draw_sphere_wires(center, radius, None, color);

        set_default_camera();
        draw_text(
            "Arrows slide the sphere. Red means the step was shortened.",
            16.0,
            28.0,
            22.0,
            BLACK,
        );
        draw_text(
            "The column is a capsule. A touch does not cancel the tangent.",
            16.0,
            54.0,
            22.0,
            DARKGRAY,
        );

        next_frame().await
    }
}
