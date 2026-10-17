//! Run `cargo run --features gltf --example gltf_textured -- model.glb`.
//! With no path, displays the small embedded textured-quad fixture.

#[cfg(feature = "gltf")]
use macroquad::prelude::*;

#[cfg(feature = "gltf")]
#[macroquad::main("Textured glTF")]
async fn main() {
    let bytes = if let Some(path) = std::env::args().nth(1) {
        load_file(&path).await.expect("read GLB")
    } else {
        include_bytes!("assets/textured_quad.glb").to_vec()
    };
    let model = GltfModel::from_bytes(&bytes).expect("load textured GLB");
    let center = (model.bounds_min + model.bounds_max) * 0.5;
    let radius = (model.bounds_max - model.bounds_min).length().max(1.0);
    loop {
        clear_background(Color::new(0.12, 0.15, 0.19, 1.0));
        let angle = get_time() as f32 * 0.20;
        set_camera(&Camera3D {
            position: center + vec3(angle.sin() * radius, radius * 0.2, angle.cos() * radius),
            target: center,
            up: Vec3::Y,
            z_near: (radius * 0.001).max(0.001),
            z_far: radius * 10.0,
            ..Default::default()
        });
        // Default rendering handles textures and material base colors. For
        // lighting and alpha, iterate model.parts and inspect part.surface.
        model.draw();
        set_default_camera();
        draw_text(
            &format!(
                "{} triangles | {} textures | {} mesh chunks",
                model.stats.triangles, model.stats.textures, model.stats.draw_calls
            ),
            18.0,
            28.0,
            22.0,
            WHITE,
        );
        next_frame().await;
    }
}

#[cfg(not(feature = "gltf"))]
fn main() {
    eprintln!("Run with --features gltf");
}
