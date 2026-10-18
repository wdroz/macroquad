//! Draw the 3D collision primitives as wireframes.
//!
//! `draw_aabb`, `draw_ray`, `draw_plane_wires`, `draw_triangle_wires`,
//! `draw_obb`, and `draw_capsule_wires` use `draw_line_3d`, `draw_cube_wires`,
//! and `draw_sphere_wires`. `draw_plane` is still the textured XZ quad.

use macroquad::prelude::*;

#[macroquad::main("Debug draw")]
async fn main() {
    let aabb = Aabb::from_center_size(vec3(-2.2, 0.5, 0.0), vec3(1.2, 1.0, 1.2));
    let obb = Obb::new(
        vec3(0.0, 0.55, 0.0),
        vec3(0.7, 0.35, 0.35),
        Quat::from_rotation_y(0.6),
    );
    let capsule = Capsule::new(vec3(2.0, 0.45, 0.0), vec3(2.0, 1.35, 0.0), 0.35);
    let triangle = Triangle::new(
        vec3(-0.6, 0.0, -1.8),
        vec3(0.8, 0.0, -1.8),
        vec3(0.1, 1.2, -1.8),
    );
    let plane = Plane::from_points(
        vec3(-1.0, 0.2, 1.6),
        vec3(1.0, 0.2, 1.6),
        vec3(0.0, 1.4, 2.2),
    )
    .unwrap();
    let ray = Ray::new(vec3(-3.5, 0.4, 2.5), vec3(1.0, 0.05, -0.35));

    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }

        clear_background(LIGHTGRAY);

        set_camera(&Camera3D {
            position: vec3(6.0, 5.0, 7.0),
            up: Vec3::Y,
            target: vec3(0.0, 0.5, 0.0),
            ..Default::default()
        });

        draw_grid(20, 1., BLACK, GRAY);
        draw_aabb(aabb, DARKGREEN);
        draw_obb(obb, BLUE);
        draw_capsule_wires(capsule, ORANGE);
        draw_triangle_wires(triangle, PURPLE);
        draw_plane_wires(
            plane,
            vec3(0.0, 0.8, 1.9),
            2.0,
            Color::new(0.2, 0.35, 0.75, 1.0),
        );
        draw_ray(ray, 6.0, YELLOW);
        draw_sphere(ray.point_at(6.0), 0.08, None, YELLOW);

        set_default_camera();
        draw_text(
            "Wireframes: box, oriented box, capsule, triangle, plane, ray.",
            16.0,
            28.0,
            22.0,
            BLACK,
        );
        draw_text("Escape quits.", 16.0, 54.0, 22.0, DARKGRAY);

        next_frame().await
    }
}
