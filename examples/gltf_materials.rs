//! Run `cargo run --features gltf --example gltf_materials -- model.glb`.
//! Without a path, shows the embedded checkerboard with opt-in material passes.

#[cfg(feature = "gltf")]
use macroquad::prelude::*;

#[cfg(feature = "gltf")]
#[macroquad::main("glTF material passes")]
async fn main() {
    let bytes = if let Some(path) = std::env::args().nth(1) {
        load_file(&path).await.expect("read GLB")
    } else {
        include_bytes!("assets/textured_quad.glb").to_vec()
    };
    let warnings = gltf_material_warnings(&bytes).expect("inspect GLB");
    for warning in &warnings {
        println!("{warning:?}");
    }
    let cpu = GltfModel::from_bytes_with_options(
        &bytes,
        GltfLoadOptions {
            max_texture_size: Some(512),
            ..Default::default()
        },
    )
    .expect("load GLB");
    drop(bytes);
    let model = RetainedGltfModel::from_model(cpu, false).expect("upload geometry");
    let mut renderer = GltfMaterialRenderer::new().expect("material shaders");
    let center = (model.bounds_min + model.bounds_max) * 0.5;
    let radius = (model.bounds_max - model.bounds_min).length().max(1.);

    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        clear_background(Color::new(0.06, 0.08, 0.12, 1.));
        let angle = get_time() as f32 * 0.15;
        let camera = Camera3D {
            position: center
                + vec3(
                    angle.sin() * radius * 2.,
                    radius * 0.3,
                    angle.cos() * radius * 2.,
                ),
            target: center,
            up: Vec3::Y,
            z_near: (radius * 0.001).max(0.001),
            z_far: radius * 20.,
            ..Default::default()
        };
        set_camera(&camera);
        let mut frame = renderer
            .begin(camera.position, camera.target - camera.position)
            .unwrap();
        // All instances share one collection, including all blended chunks.
        frame.submit_retained(&model, Mat4::IDENTITY).unwrap();
        frame
            .submit_retained(
                &model,
                Mat4::from_translation(vec3(radius * 0.7, 0., -radius * 0.2)),
            )
            .unwrap();
        let stats = frame.draw();
        set_default_camera();
        draw_text(
            "Base + emissive preview; no lighting or refraction",
            18.,
            28.,
            22.,
            WHITE,
        );
        draw_text(
            format!(
                "Opaque {} | mask {} | blend {} | {} warnings (console)",
                stats.opaque_draws,
                stats.mask_draws,
                stats.blend_draws,
                warnings.len()
            ),
            18.,
            54.,
            20.,
            LIGHTGRAY,
        );
        draw_text("Escape: exit", 18., 80., 18., LIGHTGRAY);
        next_frame().await;
    }
}

#[cfg(not(feature = "gltf"))]
fn main() {
    eprintln!("Run with --features gltf");
}
