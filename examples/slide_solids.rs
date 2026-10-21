//! Slide a sphere or a rigid two-sphere hull past a rock, column and wall.
//!
//! Arrow keys move on the XZ plane. Space switches between `slide_sphere_solids`
//! and `slide_spheres_solids`; R resets the position. Both use the same solids
//! and retain floor tangents. The hull adds one returned translation to its
//! owner, so the leading sphere stops both members without changing their
//! separation. Rotation remains fixed. Red marks shortened horizontal motion.

use macroquad::prelude::*;

#[macroquad::main("Slide solids")]
async fn main() {
    let floor = Aabb::from_center_size(vec3(0.0, -0.5, 0.0), vec3(16.0, 1.0, 16.0));
    let rock = Sphere::new(vec3(1.6, 0.55, 0.4), 0.55);
    let column = Capsule::new(vec3(-1.4, 0.45, -1.2), vec3(-1.4, 1.7, -1.2), 0.4);
    let wall = Aabb::from_center_size(vec3(3.2, 1.0, 0.0), vec3(0.3, 2.0, 10.0));
    let radius = 0.4;
    let start = vec3(0.0, radius, 2.2);
    let mut center = start;
    let mut hull = true;
    let offsets = [vec3(-0.65, 0.0, 0.0), vec3(0.65, 0.0, 0.0)];

    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        if is_key_pressed(KeyCode::Space) {
            hull = !hull;
            center = start;
        }
        if is_key_pressed(KeyCode::R) {
            center = start;
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
        let translation = if hull {
            let centers = offsets.map(|offset| center + offset);
            slide_spheres_solids(
                &centers,
                radius,
                delta,
                &[floor, wall],
                &[],
                &[rock],
                &[column],
            )
        } else {
            slide_sphere_solids(
                center,
                radius,
                delta,
                &[floor, wall],
                &[],
                &[rock],
                &[column],
            ) - center
        };
        center += translation;
        let horizontal_error = vec2(translation.x - delta.x, translation.z - delta.z);
        let shortened = horizontal_error.length_squared() > 1e-8;

        clear_background(LIGHTGRAY);
        set_camera(&Camera3D {
            position: vec3(-5.5, 4.8, 6.2),
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
        draw_cube(wall.center(), wall.size(), None, GRAY);
        let color = if shortened { RED } else { ORANGE };
        if hull {
            let centers = offsets.map(|offset| center + offset);
            for member in centers {
                draw_sphere_wires(member, radius, None, color);
            }
            draw_line_3d(centers[0], centers[1], color);
        } else {
            draw_sphere_wires(center, radius, None, color);
        }

        set_default_camera();
        draw_text(
            if hull {
                "Rigid hull: both spheres share one translation."
            } else {
                "Single sphere: the original slide_sphere_solids API."
            },
            16.0,
            28.0,
            22.0,
            BLACK,
        );
        draw_text(
            "Arrows move. Space switches body. R resets. Red = blocked sideways.",
            16.0,
            54.0,
            22.0,
            DARKGRAY,
        );

        next_frame().await
    }
}
