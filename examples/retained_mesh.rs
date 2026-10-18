//! Upload one immutable mesh, then reuse its buffers with different transforms.
//! Immediate geometry and text still draw in their submitted order.

use macroquad::prelude::*;

fn diamond() -> Mesh {
    let vertices = [
        (vec3(0., 1., 0.), YELLOW),
        (vec3(1., 0., 0.), ORANGE),
        (vec3(0., 0., 1.), SKYBLUE),
        (vec3(-1., 0., 0.), BLUE),
        (vec3(0., 0., -1.), PURPLE),
        (vec3(0., -1., 0.), PINK),
    ]
    .into_iter()
    .map(|(position, color)| Vertex::new2(position, Vec2::ZERO, color))
    .collect();
    Mesh {
        vertices,
        indices: vec![
            0, 2, 1, 0, 3, 2, 0, 4, 3, 0, 1, 4, 5, 1, 2, 5, 2, 3, 5, 3, 4, 5, 4, 1,
        ],
        texture: None,
    }
}

#[macroquad::main("Retained GPU mesh")]
async fn main() {
    // The temporary CPU mesh is dropped here. Cloning the retained handle
    // shares its uploaded vertex/index buffers instead of uploading again.
    let mesh = RetainedMesh::from_mesh(&diamond()).unwrap();
    let shared = mesh.clone();

    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        let time = get_time() as f32;
        clear_background(Color::from_rgba(19, 25, 38, 255));
        set_camera(&Camera3D {
            position: vec3(8., 7., 11.),
            target: vec3(0., 0.7, 0.),
            up: Vec3::Y,
            ..Default::default()
        });

        draw_grid(20, 1., DARKGRAY, GRAY);
        for row in 0..2 {
            for column in 0..6 {
                let phase = time + column as f32 * 0.4 + row as f32;
                let position = vec3(
                    (column as f32 - 2.5) * 1.8,
                    0.9 + phase.sin() * 0.2,
                    (row as f32 - 0.5) * 2.8,
                );
                let transform = Mat4::from_scale_rotation_translation(
                    Vec3::splat(0.65),
                    Quat::from_rotation_y(phase) * Quat::from_rotation_z(0.2),
                    position,
                );
                draw_retained_mesh(if row == 0 { &mesh } else { &shared }, transform);
            }
        }

        set_default_camera();
        draw_rectangle(0., 0., screen_width(), 76., Color::new(0., 0., 0., 0.7));
        draw_text(
            "One GPU mesh, twelve transformed draws",
            20.,
            30.,
            26.,
            WHITE,
        );
        draw_text(
            "The grid and this overlay use immediate drawing. Escape exits.",
            20.,
            57.,
            20.,
            LIGHTGRAY,
        );
        next_frame().await;
    }
}
