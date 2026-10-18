//! Color boxes by a second camera's frustum.
//!
//! The yellow wireframe is a culling camera, not the camera you look through.
//! Green boxes intersect that frustum. Gray boxes are outside it. Left and
//! right yaw the culling camera. Space hides the gray boxes. Escape quits.

use macroquad::prelude::*;

fn culler(yaw: f32) -> Camera3D {
    let distance = 9.0;
    Camera3D {
        position: vec3(distance * yaw.sin(), 3.2, distance * yaw.cos()),
        target: vec3(0.0, 0.4, 0.0),
        up: Vec3::Y,
        fovy: 40.0_f32.to_radians(),
        aspect: Some(1.0),
        z_near: 1.0,
        z_far: 14.0,
        ..Default::default()
    }
}

fn frustum_corners(camera: &Camera3D) -> [Vec3; 8] {
    let forward = (camera.target - camera.position).normalize();
    let right = forward.cross(camera.up).normalize();
    let up = right.cross(forward);
    let aspect = camera.aspect.unwrap_or(1.0);
    let tan_y = (camera.fovy * 0.5).tan();
    let tan_x = tan_y * aspect;
    let mut corners = [Vec3::ZERO; 8];
    let mut index = 0;
    for depth in [camera.z_near, camera.z_far] {
        let center = camera.position + forward * depth;
        let half_width = tan_x * depth;
        let half_height = tan_y * depth;
        for (side, lift) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            corners[index] = center + right * (half_width * side) + up * (half_height * lift);
            index += 1;
        }
    }
    corners
}

fn draw_frustum(corners: &[Vec3; 8]) {
    for index in 0..4 {
        let next = (index + 1) % 4;
        draw_line_3d(corners[index], corners[next], YELLOW);
        draw_line_3d(corners[index + 4], corners[next + 4], YELLOW);
        draw_line_3d(corners[index], corners[index + 4], YELLOW);
    }
}

#[macroquad::main("Frustum cull")]
async fn main() {
    let mut boxes = Vec::new();
    for x in -3..=3 {
        for z in -2..=2 {
            boxes.push(Aabb::from_center_size(
                vec3(x as f32 * 1.6, 0.4, z as f32 * 1.6),
                vec3(0.7, 0.8, 0.7),
            ));
        }
    }
    let view = Camera3D {
        position: vec3(12.0, 9.0, 12.0),
        target: vec3(0.0, 0.4, 0.0),
        up: Vec3::Y,
        ..Default::default()
    };
    let mut yaw = 0.4;
    let mut hide_outside = false;

    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        if is_key_down(KeyCode::Left) {
            yaw -= 0.02;
        }
        if is_key_down(KeyCode::Right) {
            yaw += 0.02;
        }
        if is_key_pressed(KeyCode::Space) {
            hide_outside = !hide_outside;
        }

        let culler = culler(yaw);
        let Some(frustum) = culler.frustum() else {
            next_frame().await;
            continue;
        };

        clear_background(LIGHTGRAY);
        set_camera(&view);
        draw_grid(20, 1., BLACK, GRAY);
        draw_frustum(&frustum_corners(&culler));
        draw_sphere(culler.position, 0.15, None, YELLOW);
        for box_aabb in &boxes {
            let inside = frustum.intersects_aabb(box_aabb);
            if !inside && hide_outside {
                continue;
            }
            let color = if inside { DARKGREEN } else { GRAY };
            draw_cube_wires(box_aabb.center(), box_aabb.size(), color);
        }

        set_default_camera();
        draw_text(
            "Left and right yaw the yellow frustum. Green boxes intersect it.",
            16.0,
            28.0,
            22.0,
            BLACK,
        );
        draw_text(
            "Space hides boxes outside the frustum. Escape quits.",
            16.0,
            54.0,
            22.0,
            DARKGRAY,
        );

        next_frame().await
    }
}
