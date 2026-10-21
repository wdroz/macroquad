//! Self-contained morph geometry, queue lifetime and material regressions.

use macroquad::{prelude::*, telemetry};

fn glb(reflected: bool, normals: bool, defaults: f32) -> Vec<u8> {
    let mut bin = Vec::new();
    for values in [
        vec![
            [-0.22_f32, -0.22, 0.],
            [0.22, -0.22, 0.],
            [0.22, 0.22, 0.],
            [-0.22, 0.22, 0.],
        ],
        vec![[0., 0., 1.]; 4],
        vec![[0.3, 0., 0.4]; 4],
        vec![[0., 1., 0.]; 4],
    ] {
        for value in values {
            for component in value {
                bin.extend(component.to_le_bytes());
            }
        }
    }
    for index in [0_u16, 1, 2, 0, 2, 3] {
        bin.extend(index.to_le_bytes());
    }
    let mut doc = serde_json::json!({
        "asset": {"version":"2.0"}, "scene":0, "scenes":[{"nodes":[0]}],
        "nodes":[{"name":"face", "mesh":0, "scale":[if reflected {-1.} else {1.}, 1., 1.]}],
        "meshes":[{"weights":[defaults], "primitives":[{
            "attributes":{"POSITION":0,"NORMAL":1}, "indices":4, "material":0,
            "targets":[{"POSITION":2,"NORMAL":3}]
        }]}],
        "materials":[{"pbrMetallicRoughness":{"baseColorFactor":[1.,1.,1.,1.]}}],
        "buffers":[{"byteLength":bin.len()}],
        "bufferViews":[
            {"buffer":0,"byteOffset":0,"byteLength":48},
            {"buffer":0,"byteOffset":48,"byteLength":48},
            {"buffer":0,"byteOffset":96,"byteLength":48},
            {"buffer":0,"byteOffset":144,"byteLength":48},
            {"buffer":0,"byteOffset":192,"byteLength":12}
        ],
        "accessors":[
            {"bufferView":0,"componentType":5126,"count":4,"type":"VEC3","min":[-0.22,-0.22,0.],"max":[0.22,0.22,0.]},
            {"bufferView":1,"componentType":5126,"count":4,"type":"VEC3"},
            {"bufferView":2,"componentType":5126,"count":4,"type":"VEC3"},
            {"bufferView":3,"componentType":5126,"count":4,"type":"VEC3"},
            {"bufferView":4,"componentType":5123,"count":6,"type":"SCALAR"}
        ]
    });
    if !normals {
        doc["meshes"][0]["primitives"][0]["attributes"]
            .as_object_mut()
            .unwrap()
            .remove("NORMAL");
        doc["meshes"][0]["primitives"][0]["targets"][0]
            .as_object_mut()
            .unwrap()
            .remove("NORMAL");
    }
    let mut json = serde_json::to_vec(&doc).unwrap();
    json.resize((json.len() + 3) & !3, b' ');
    let mut bytes = Vec::new();
    bytes.extend(b"glTF");
    bytes.extend(2_u32.to_le_bytes());
    bytes.extend(((28 + json.len() + bin.len()) as u32).to_le_bytes());
    bytes.extend((json.len() as u32).to_le_bytes());
    bytes.extend(b"JSON");
    bytes.extend(json);
    bytes.extend((bin.len() as u32).to_le_bytes());
    bytes.extend(b"BIN\0");
    bytes.extend(bin);
    bytes
}

fn read(target: &RenderTarget) -> Image {
    unsafe { get_internal_gl().flush() };
    target.texture.get_texture_data()
}

fn pixel(image: &Image, x: usize, expected: [u8; 4], label: &str) {
    let offset = (32 * image.width as usize + x) * 4;
    let actual = &image.bytes[offset..offset + 4];
    for component in 0..4 {
        assert!(
            actual[component].abs_diff(expected[component]) <= 2,
            "{label} pixel {x}: expected {expected:?}, got {actual:?}"
        );
    }
}

fn close(actual: Vec3, expected: Vec3) {
    assert!(
        actual.distance(expected) < 1e-5,
        "{actual:?} != {expected:?}"
    );
}

fn no_uploads() {
    let counts = telemetry::renderer_diagnostics().counters;
    assert_eq!(counts.retained_buffer_uploads, 0);
    assert_eq!(counts.retained_buffer_upload_bytes, 0);
}

pub async fn run() {
    let target = render_target_ex(
        64,
        64,
        RenderTargetParams {
            depth: true,
            ..Default::default()
        },
    );
    let camera = Camera3D {
        position: vec3(0., 0., 4.),
        target: Vec3::ZERO,
        up: Vec3::Y,
        projection: Projection::Orthographics,
        fovy: 2.,
        aspect: Some(1.),
        z_near: 0.1,
        z_far: 10.,
        render_target: Some(target.clone()),
        ..Default::default()
    };
    set_camera(&camera);
    telemetry::set_renderer_diagnostics_enabled(true);
    let mut renderer = GltfMaterialRenderer::new().unwrap();
    for reflected in [false, true] {
        for normals in [true, false] {
            let bytes = glb(reflected, normals, 0.25);
            let settings = GltfLoadSettings {
                options: GltfLoadOptions {
                    mesh_limits: GltfMeshLimits {
                        vertices: 3,
                        indices: 3,
                    },
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut loader = GltfModelLoader::new_with_morph(&bytes, settings).unwrap();
            assert!(loader.take_morph_model().unwrap().is_none());
            while loader.poll().unwrap().stage != GltfLoadStage::Complete {}
            let poses = loader.take_pose().unwrap(); // Sidecars are independent.
            let source = loader.take_morph_model().unwrap().unwrap();
            assert!(loader.take_morph_model().unwrap().is_none());
            assert!(loader.take_model().is_none());
            assert_eq!(source.pose().rest_world(0), poses.rest_world(0));
            let mut first = source.instantiate();
            let mut second = first.clone();
            assert_eq!(first.weights(0), Some([0.25].as_slice()));
            assert_eq!(first.model().parts.len(), 2);
            assert!(first.model().cpu_model().is_none());
            let sign = if reflected { -1. } else { 1. };
            close(
                first.model().world_bounds(Mat4::IDENTITY).unwrap().center(),
                vec3(sign * 0.075, 0., 0.1),
            );
            telemetry::reset_renderer_diagnostics();
            assert!(!first.set_weights(0, &[0.25]).unwrap());
            assert!(first.set_weights(0, &[f32::NAN]).is_err());
            assert!(first.set_weights(0, &[]).is_err());
            assert!(first.set_weights(usize::MAX, &[0.]).is_err());
            no_uploads();
            assert!(first.set_weights(0, &[1.]).unwrap());
            assert_eq!(
                telemetry::renderer_diagnostics()
                    .counters
                    .retained_buffer_uploads,
                4
            );
            assert_eq!(second.weights(0), Some([0.25].as_slice()));
            close(
                second
                    .model()
                    .world_bounds(Mat4::IDENTITY)
                    .unwrap()
                    .center(),
                vec3(sign * 0.075, 0., 0.1),
            );
            close(
                first.model().world_bounds(Mat4::IDENTITY).unwrap().center(),
                vec3(sign * 0.3, 0., 0.4),
            );
            // Imported inverse transpose and updated normals reach the existing
            // material path; missing authored normals regenerate geometric +Z.
            clear_background(BLACK);
            let mut frame = renderer.begin(camera.position, -Vec3::Z).unwrap();
            frame.set_output(GltfColorOutput::Linear);
            frame
                .set_lighting(Some(GltfLighting {
                    hemisphere_up: Vec3::Y,
                    sky_color: Vec3::ZERO,
                    ground_color: Vec3::ZERO,
                    directional_direction: Vec3::Z,
                    directional_color: Vec3::ONE,
                }))
                .unwrap();
            frame
                .submit_retained(
                    first.model(),
                    Mat4::from_translation(vec3(-sign * 0.3, 0., 0.)),
                )
                .unwrap();
            assert_eq!(frame.draw().opaque_draws, 2);
            let value = if normals { 180 } else { 255 };
            pixel(
                &read(&target),
                32,
                [value, value, value, 255],
                "deformed normal and reflected winding",
            );
            // Replacement poses consume the same deformed baked snapshot.
            let mut pose = source.pose().node(0).unwrap();
            pose.set_world(Mat4::from_scale_rotation_translation(
                vec3(1., 0.5, 2.),
                Quat::IDENTITY,
                vec3(-0.3, 0., 0.),
            ))
            .unwrap();
            close(
                first.model().parts[0]
                    .posed_bounds(&pose, Mat4::IDENTITY)
                    .unwrap()
                    .center(),
                vec3(0., 0., 0.8),
            );
            clear_background(BLACK);
            let mut frame = renderer.begin(camera.position, -Vec3::Z).unwrap();
            frame.set_output(GltfColorOutput::Linear);
            frame
                .set_lighting(Some(GltfLighting {
                    hemisphere_up: Vec3::Y,
                    sky_color: Vec3::ZERO,
                    ground_color: Vec3::ZERO,
                    directional_direction: Vec3::Z,
                    directional_color: Vec3::ONE,
                }))
                .unwrap();
            for part in &first.model().parts {
                frame.submit_retained_part_posed(part, &pose).unwrap();
            }
            assert_eq!(frame.draw().opaque_draws, 2);
            let value = if normals { 62 } else { 255 };
            pixel(
                &read(&target),
                32,
                [value, value, value, 255],
                "deformed replacement pose normals and bounds",
            );
            // Deformed bounds, rather than baked rest bounds, drive culling.
            let frustum = camera.frustum().unwrap();
            first.set_weights(0, &[8.]).unwrap();
            second.set_weights(0, &[0.]).unwrap();
            assert_eq!(first.model().draw_culled(Mat4::IDENTITY, &frustum), 0);
            assert_eq!(second.model().draw_culled(Mat4::IDENTITY, &frustum), 2);
            read(&target);
            assert!(first.reset_weights(0).unwrap());
            assert_eq!(first.weights(0), Some([0.25].as_slice()));
            assert!(!first.reset_weights(0).unwrap());
            // Direct retained submission captures pre-update buffers. The old
            // white square must stay on the left after a same-frame update.
            first.set_weights(0, &[0.]).unwrap();
            clear_background(BLACK);
            first
                .model()
                .draw(Mat4::from_translation(vec3(-0.5, 0., 0.)));
            first.set_weights(0, &[1.]).unwrap();
            first
                .model()
                .draw(Mat4::from_translation(vec3(0.5 - sign * 0.3, 0., 0.)));
            drop(first);
            drop(second);
            drop(source);
            drop(loader);
            retire_unused_render_resources();
            let image = read(&target);
            pixel(
                &image,
                16,
                [255, 255, 255, 255],
                "queued old geometry survives update and final drop",
            );
            pixel(
                &image,
                48,
                [255, 255, 255, 255],
                "queued new geometry survives final drop",
            );
        }
    }

    // Captured retained metadata uses current centers for global alpha sorting.
    let source = GltfMorphModel::from_bytes(&glb(false, true, 0.)).unwrap();
    let mut near = source.instantiate();
    let far = source.instantiate();
    near.set_weights(0, &[1.]).unwrap();
    let mut red = near.model().clone();
    let mut blue = far.model().clone();
    for part in &mut red.parts {
        part.surface.base_color = [1., 0., 0., 0.5];
        part.surface.alpha_mode = GltfAlphaMode::Blend;
    }
    for part in &mut blue.parts {
        part.surface.base_color = [0., 0., 1., 0.5];
        part.surface.alpha_mode = GltfAlphaMode::Blend;
    }
    for reverse in [false, true] {
        clear_background(BLACK);
        let mut frame = renderer.begin(camera.position, -Vec3::Z).unwrap();
        frame.set_output(GltfColorOutput::Linear);
        if reverse {
            frame.submit_retained(&blue, Mat4::IDENTITY).unwrap();
            frame
                .submit_retained(&red, Mat4::from_translation(vec3(-0.3, 0., 0.)))
                .unwrap();
        } else {
            frame
                .submit_retained(&red, Mat4::from_translation(vec3(-0.3, 0., 0.)))
                .unwrap();
            frame.submit_retained(&blue, Mat4::IDENTITY).unwrap();
        }
        assert_eq!(frame.draw().blend_draws, 2);
        pixel(
            &read(&target),
            32,
            [128, 0, 64, 255],
            "deformed center global transparency ordering",
        );
    }
    telemetry::reset_renderer_diagnostics();
    for _ in 0..3 {
        assert!(!near.set_weights(0, &[1.]).unwrap());
        near.model().draw(Mat4::IDENTITY);
        read(&target);
    }
    no_uploads();
    set_default_camera();
    clear_background(GREEN);
    next_frame().await;
    macroquad::logging::info!("GLTF_MORPH_FIXTURE_PASS");
}
