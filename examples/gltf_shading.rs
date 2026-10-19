//! A lit quad that samples a normal map and a metallic-roughness map.
//!
//! The left half is a rough dielectric with a flat normal. The right half is
//! a rough metal whose normal leans toward +X. Escape exits. Without
//! `--features gltf` the example prints a reminder and exits.

#[cfg(feature = "gltf")]
use macroquad::prelude::*;

#[cfg(feature = "gltf")]
fn nearest(width: u16, height: u16, bytes: &[u8]) -> Texture2D {
    let texture = Texture2D::from_rgba8(width, height, bytes);
    texture.set_filter(FilterMode::Nearest);
    texture
}

#[cfg(feature = "gltf")]
fn scene() -> (GltfModel, GltfModelMaterialMaps) {
    let vertices = [
        ([-1.0_f32, -1.0, 0.0], [0.0_f32, 1.0]),
        ([1.0, -1.0, 0.0], [1.0, 1.0]),
        ([1.0, 1.0, 0.0], [1.0, 0.0]),
        ([-1.0, 1.0, 0.0], [0.0, 0.0]),
    ];
    let mesh_vertices = vertices
        .iter()
        .map(|(position, uv)| Vertex {
            position: Vec3::from_array(*position),
            uv: Vec2::from_array(*uv),
            color: WHITE.into(),
            normal: Vec4::Z,
        })
        .collect();
    let part = GltfDrawMesh {
        mesh: Mesh {
            vertices: mesh_vertices,
            indices: vec![0, 1, 2, 0, 2, 3],
            texture: None,
        },
        emissive_texture: None,
        surface: GltfSurface {
            name: Some("panel".into()),
            base_color: [0.85, 0.35, 0.15, 1.0],
            emissive: [0.0; 3],
            metallic: 1.0,
            roughness: 1.0,
            transmission: 0.0,
            alpha_mode: GltfAlphaMode::Opaque,
            alpha_cutoff: 0.5,
            double_sided: false,
            unlit: false,
        },
        node: 0,
        name: "panel".into(),
        path: "panel".into(),
        center: Vec3::ZERO,
        bounds_min: vec3(-1.0, -1.0, 0.0),
        bounds_max: vec3(1.0, 1.0, 0.0),
    };
    let uvs = vertices.map(|(_, uv)| Vec2::from_array(uv)).to_vec();
    // Two texels, nearest filtered. Left: flat normal, dielectric. Right: +X
    // normal, metal. Green is roughness and blue is metallic; both stay linear.
    let maps = GltfPartMaterialMaps {
        normal: Some(GltfDrawMaterialMap {
            texture: nearest(2, 1, &[128, 128, 255, 255, 255, 128, 128, 255]),
            tex_coord: 0,
            transform: GltfUvTransform::default(),
            strength: 1.0,
            uvs: uvs.clone(),
        }),
        metallic_roughness: Some(GltfDrawMaterialMap {
            texture: nearest(2, 1, &[0, 255, 0, 255, 0, 255, 255, 255]),
            tex_coord: 0,
            transform: GltfUvTransform::default(),
            strength: 1.0,
            uvs,
        }),
        tangents: vec![vec4(1.0, 0.0, 0.0, 1.0); 4],
        material_index: Some(0),
        material_name: Some("panel".into()),
    };
    let model = GltfModel {
        bounds_min: part.bounds_min,
        bounds_max: part.bounds_max,
        stats: GltfModelStats::default(),
        root_names: vec!["panel".into()],
        parts: vec![part],
    };
    (model, GltfModelMaterialMaps { parts: vec![maps] })
}

#[cfg(feature = "gltf")]
#[macroquad::main("glTF normal and roughness")]
async fn main() {
    let (model, maps) = scene();
    let mut renderer = GltfMaterialRenderer::new().expect("shaded material shaders");
    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        clear_background(Color::new(0.05, 0.06, 0.08, 1.0));
        let camera = Camera3D {
            position: vec3(0.2, 0.35, 2.6),
            target: Vec3::ZERO,
            up: Vec3::Y,
            ..Default::default()
        };
        set_camera(&camera);
        let angle = get_time() as f32;
        let mut frame = renderer
            .begin(camera.position, camera.target - camera.position)
            .unwrap();
        frame
            .set_lighting(Some(GltfLighting {
                hemisphere_up: Vec3::Y,
                sky_color: vec3(0.25, 0.28, 0.32),
                ground_color: vec3(0.05, 0.04, 0.04),
                directional_direction: vec3(angle.sin(), 0.35, angle.cos()),
                directional_color: Vec3::ONE,
            }))
            .unwrap();
        frame.submit_shaded(&model, &maps, Mat4::IDENTITY).unwrap();
        frame.draw();
        set_default_camera();
        draw_text(
            "Left: rough dielectric, flat normal. Right: rough metal, +X normal.",
            18.0,
            28.0,
            22.0,
            WHITE,
        );
        draw_text("Escape: exit", 18.0, 54.0, 20.0, LIGHTGRAY);
        next_frame().await;
    }
}

#[cfg(not(feature = "gltf"))]
fn main() {
    eprintln!("Run with --features gltf");
}
