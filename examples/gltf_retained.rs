//! Run `cargo run --features gltf --example gltf_retained`.
//! One uploaded textured fixture, shared by three moving instances.

#[cfg(feature = "gltf")]
use macroquad::prelude::*;

#[cfg(feature = "gltf")]
#[macroquad::main("Retained glTF instances")]
async fn main() {
    let cpu = GltfModel::from_bytes(include_bytes!("assets/textured_quad.glb")).unwrap();
    let center = (cpu.bounds_min + cpu.bounds_max) * 0.5;
    let scale = 1.4 / (cpu.bounds_max - cpu.bounds_min).length().max(0.001);
    // `false` releases the CPU vertices and indices after upload.
    let model = RetainedGltfModel::from_model(cpu, false).unwrap();
    let instances = [model.clone(), model.clone(), model];

    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        clear_background(Color::new(0.06, 0.08, 0.12, 1.));
        let camera = Camera3D {
            position: vec3(0., 1., 6.),
            target: Vec3::ZERO,
            up: Vec3::Y,
            ..Default::default()
        };
        set_camera(&camera);
        let frustum = camera.frustum().unwrap();
        let time = get_time() as f32;
        let mut submitted = 0;
        for (i, instance) in instances.iter().enumerate() {
            let position = vec3((i as f32 - 1.) * 1.8, (time + i as f32).sin() * 0.3, 0.);
            let reflection = if i == 2 { -1. } else { 1. };
            let transform = Mat4::from_scale_rotation_translation(
                vec3(reflection, 1., 1.) * scale,
                Quat::from_rotation_y(time * 0.35),
                position,
            ) * Mat4::from_translation(-center);
            // The default material is unlit and does not cull faces, so this
            // also draws the reflected instance. Custom lit/culling materials
            // use GltfModelTransform::new(transform) for their normal uniform
            // and mirrored front-face pipeline choice.
            submitted += instance.draw_culled(transform, &frustum);
        }
        set_default_camera();
        draw_text(
            "One upload, three shared glTF instances",
            20.,
            30.,
            26.,
            WHITE,
        );
        draw_text(
            format!("{submitted} visible chunks | CPU geometry released | Escape exits"),
            20.,
            58.,
            20.,
            LIGHTGRAY,
        );
        next_frame().await;
    }
}

#[cfg(not(feature = "gltf"))]
fn main() {
    eprintln!("Run with --features gltf");
}
