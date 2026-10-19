//! Small imported-part/hierarchy regressions shared by OpenGL and WebGL.
//! Positions and lighting expectations are calculated from the authored local
//! quad and replacement matrix, independently of the pose's additional matrix.

use macroquad::prelude::*;
use macroquad::telemetry;

fn document(reflected_rest: bool) -> (serde_json::Value, Vec<u8>) {
    let mut bin = Vec::new();
    for point in [
        [-0.3_f32, -0.3, 0.],
        [0.3, -0.3, 0.],
        [0.3, 0.3, 0.],
        [-0.3, 0.3, 0.],
    ] {
        for component in point {
            bin.extend(component.to_le_bytes());
        }
    }
    // A smooth normal deliberately differs from the geometric +Z face.
    // Its three components reveal both nonuniform scales and reflection.
    let normal = vec3(1., 2., 3.).normalize();
    for _ in 0..4 {
        for component in normal.to_array() {
            bin.extend(component.to_le_bytes());
        }
    }
    for index in [0_u16, 1, 2, 0, 2, 3] {
        bin.extend(index.to_le_bytes());
    }
    let half = std::f32::consts::FRAC_1_SQRT_2;
    let yaw = std::f32::consts::PI / 6.;
    let document = serde_json::json!({
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0, 2]}],
        "nodes": [
            {"name": "parent", "translation": [2., 1., 0.1], "children": [1]},
            {"name": "moved_part", "mesh": 0, "translation": [1., 0.5, 0.],
             "rotation": [0., 0., half, half],
             "scale": [if reflected_rest { -2. } else { 2. }, 1., 0.5]},
            {"name": "player_spawn", "translation": [-0.5, 0., 0.],
             "rotation": [0., (yaw * 0.5).sin(), 0., (yaw * 0.5).cos()]}
        ],
        "meshes": [{"primitives": [{
            "attributes": {"POSITION": 0, "NORMAL": 1}, "indices": 2, "material": 0
        }]}],
        "materials": [{"name": "white single-sided", "pbrMetallicRoughness": {
            "baseColorFactor": [1., 1., 1., 1.], "metallicFactor": 0.
        }}],
        "buffers": [{"byteLength": bin.len()}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 48},
            {"buffer": 0, "byteOffset": 48, "byteLength": 48},
            {"buffer": 0, "byteOffset": 96, "byteLength": 12}
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3",
             "min": [-0.3, -0.3, 0.], "max": [0.3, 0.3, 0.]},
            {"bufferView": 1, "componentType": 5126, "count": 4, "type": "VEC3"},
            {"bufferView": 2, "componentType": 5123, "count": 6, "type": "SCALAR"}
        ]
    });
    (document, bin)
}

fn encode_glb(document: serde_json::Value, bin: Vec<u8>) -> Vec<u8> {
    let mut json = serde_json::to_vec(&document).unwrap();
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

fn glb(reflected_rest: bool) -> Vec<u8> {
    let (document, bin) = document(reflected_rest);
    encode_glb(document, bin)
}

fn hierarchy_glb() -> Vec<u8> {
    let (mut document, bin) = document(false);
    document["scenes"] = serde_json::json!([{ "nodes": [0] }]);
    document["nodes"] = serde_json::json!([
        { "name": "nonuniform_parent", "translation": [3., 1., 0.1],
          "scale": [2., 0.5, 1.], "children": [1, 2] },
        { "name": "blind", "mesh": 0, "translation": [0.25, 0., 0.] },
        { "name": "prop", "mesh": 1, "translation": [-0.25, 0., 0.] }
    ]);
    let mut blue = document["meshes"][0].clone();
    blue["primitives"][0]["material"] = serde_json::json!(1);
    document["meshes"].as_array_mut().unwrap().push(blue);
    document["materials"] = serde_json::json!([
        { "name": "red blind", "alphaMode": "BLEND",
          "pbrMetallicRoughness": { "baseColorFactor": [1., 0., 0., 0.5] } },
        { "name": "blue prop", "alphaMode": "BLEND",
          "pbrMetallicRoughness": { "baseColorFactor": [0., 0., 1., 0.5] } }
    ]);
    encode_glb(document, bin)
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
            "{label}: ({x}, 32) channel {component}: expected {expected:?}, got {actual:?}"
        );
    }
}

fn same_pixels(actual: &Image, expected: &Image) {
    assert_eq!(actual.width, expected.width);
    assert_eq!(actual.height, expected.height);
    for (offset, (actual, expected)) in actual.bytes.iter().zip(&expected.bytes).enumerate() {
        assert!(
            actual.abs_diff(*expected) <= 2,
            "posed CPU/retained pixel {} channel {}: expected {expected}, got {actual}",
            offset / 4,
            offset % 4
        );
    }
}

fn close(actual: Vec3, expected: Vec3) {
    assert!(
        actual.distance(expected) < 0.0001,
        "expected {expected:?}, got {actual:?}"
    );
}

fn assert_no_uploads(retained: bool) {
    let counters = telemetry::renderer_diagnostics().counters;
    assert_eq!(counters.retained_buffer_uploads, 0);
    assert_eq!(counters.retained_buffer_upload_bytes, 0);
    if retained {
        assert_eq!(counters.streaming_buffer_uploads, 0);
        assert_eq!(counters.streaming_buffer_upload_bytes, 0);
    }
}

fn hierarchy(renderer: &mut GltfMaterialRenderer, camera: &Camera3D, target: &RenderTarget) {
    let (cpu, rest) = GltfModel::from_bytes_with_pose(&hierarchy_glb()).unwrap();
    assert_eq!(cpu.parts.len(), 2);
    let retained = RetainedGltfModel::from_model(cpu.clone(), false).unwrap();
    assert!(retained.cpu_model().is_none());
    let parent = rest.handle(0).unwrap();
    let blind = rest.handle(1).unwrap();
    let prop = rest.handle(2).unwrap();
    assert_eq!(cpu.parts[0].node, blind.index());
    assert_eq!(cpu.parts[1].node, prop.index());
    let frustum = camera.frustum().unwrap();
    let mut left = rest.hierarchy().unwrap();
    let mut right = left.clone();
    assert_eq!(left.parent(blind).unwrap(), Some(parent));
    assert_eq!(left.parent(prop).unwrap(), Some(parent));
    for part in &cpu.parts {
        let handle = rest.handle(part.node).unwrap();
        let pose = left.node_pose(handle).unwrap();
        assert_eq!(pose.transform(), Mat4::IDENTITY);
        assert!(!frustum.intersects_aabb(&part.posed_bounds(&pose, Mat4::IDENTITY).unwrap()));
    }

    let scale = vec3(0.75, 0.5, 1.);
    left.set_local(
        parent,
        GltfLocalTransform {
            translation: vec3(-0.6875, 0., 0.),
            rotation: Quat::IDENTITY,
            scale,
        },
    )
    .unwrap();
    // A local blind turn under nonuniform parent scale creates a sheared
    // world matrix. Its pivot follows the parent, without recentering bounds.
    let mut blind_local = left.local_trs(blind).unwrap();
    blind_local.rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_4);
    left.set_local(blind, blind_local).unwrap();
    close(
        left.world(blind).unwrap().w_axis.truncate(),
        vec3(-0.5, 0., 0.),
    );
    close(
        left.world(prop).unwrap().w_axis.truncate(),
        vec3(-0.875, 0., 0.),
    );
    assert_eq!(
        right.world(blind).unwrap(),
        rest.rest_world(blind.index()).unwrap()
    );
    assert_eq!(
        right.local(blind).unwrap(),
        rest.rest_local(blind.index()).unwrap()
    );
    right
        .set_local(
            parent,
            GltfLocalTransform {
                translation: vec3(0.6875, 0., 0.),
                rotation: Quat::IDENTITY,
                scale,
            },
        )
        .unwrap();
    close(
        right.world(prop).unwrap().w_axis.truncate(),
        vec3(0.5, 0., 0.),
    );

    let left_pose = left.node_pose(blind).unwrap();
    let right_pose = right.node_pose(prop).unwrap();
    // These exact extents come from the authored quad, local 45-degree
    // rotation and parent scale, independently of the pose implementation.
    let left_extent = vec3(0.225 * 2_f32.sqrt(), 0.15 * 2_f32.sqrt(), 0.);
    for (index, pose, center, extent) in [
        (0, left_pose, vec3(-0.5, 0., 0.), left_extent),
        (1, right_pose, vec3(0.5, 0., 0.), vec3(0.225, 0.15, 0.)),
    ] {
        for bounds in [
            cpu.parts[index]
                .posed_bounds(&pose, Mat4::IDENTITY)
                .unwrap(),
            retained.parts[index]
                .posed_bounds(&pose, Mat4::IDENTITY)
                .unwrap(),
        ] {
            close(bounds.min, center - extent);
            close(bounds.max, center + extent);
            assert!(frustum.intersects_aabb(&bounds));
        }
    }
    for retained_geometry in [false, true] {
        for _ in 0..2 {
            clear_background(BLACK);
            telemetry::reset_renderer_diagnostics();
            let mut frame = renderer.begin(camera.position, -Vec3::Z).unwrap();
            frame.set_output(GltfColorOutput::Linear);
            if retained_geometry {
                frame
                    .submit_retained_part_posed(&retained.parts[0], &left_pose)
                    .unwrap();
                frame
                    .submit_retained_part_posed(&retained.parts[1], &right_pose)
                    .unwrap();
            } else {
                frame.submit_part_posed(&cpu.parts[0], &left_pose).unwrap();
                frame.submit_part_posed(&cpu.parts[1], &right_pose).unwrap();
            }
            assert_eq!(frame.draw().blend_draws, 2);
            let image = read(target);
            pixel(
                &image,
                16,
                [128, 0, 0, 255],
                "parented blind follows left instance",
            );
            pixel(
                &image,
                32,
                [0, 0, 0, 255],
                "independent instances leave center clear",
            );
            pixel(
                &image,
                48,
                [0, 0, 128, 255],
                "parented prop follows right instance",
            );
            assert_no_uploads(retained_geometry);
        }
    }

    left.reset_local(blind).unwrap();
    let reset_blind = left.node_pose(blind).unwrap();
    let bounds = cpu.parts[0]
        .posed_bounds(&reset_blind, Mat4::IDENTITY)
        .unwrap();
    close(bounds.min, vec3(-0.725, -0.15, 0.));
    close(bounds.max, vec3(-0.275, 0.15, 0.));
    left.reset();
    assert_eq!(left.node_pose(blind).unwrap().transform(), Mat4::IDENTITY);
    assert_eq!(left.node_pose(prop).unwrap().transform(), Mat4::IDENTITY);
    assert_eq!(right.node_pose(prop).unwrap().world(), right_pose.world());
    assert_eq!(
        rest.rest_world(blind.index()).unwrap().w_axis.truncate(),
        vec3(3.5, 1., 0.1)
    );
    // The reset blind is offscreen again. Use its updated bounds for the
    // same application-level culling query as the visible instance above.
    clear_background(BLACK);
    telemetry::reset_renderer_diagnostics();
    let mut frame = renderer.begin(camera.position, -Vec3::Z).unwrap();
    let mut submitted = 0;
    for part in &retained.parts {
        let pose = left.node_pose(rest.handle(part.node).unwrap()).unwrap();
        if frustum.intersects_aabb(&part.posed_bounds(&pose, Mat4::IDENTITY).unwrap()) {
            frame.submit_retained_part_posed(part, &pose).unwrap();
            submitted += 1;
        }
    }
    assert_eq!(submitted, 0);
    assert_eq!(frame.draw().blend_draws, 0);
    pixel(
        &read(target),
        16,
        [0, 0, 0, 255],
        "rest reset agrees with culling",
    );
    assert_no_uploads(true);

    // Two independently edited instances share one geometry model. Changing
    // parent depth swaps the front layer. Both source orders must produce the
    // analytic global source-over result, using updated descendant centers.
    left.set_local(blind, blind_local).unwrap();
    for red_front in [false, true] {
        let red_z = if red_front { 0.25 } else { -0.25 };
        for (instance, x, z) in [(&mut left, -0.1875, red_z), (&mut right, 0.1875, -red_z)] {
            instance
                .set_local(
                    parent,
                    GltfLocalTransform {
                        translation: vec3(x, 0., z),
                        rotation: Quat::IDENTITY,
                        scale,
                    },
                )
                .unwrap();
        }
        let red = left.node_pose(blind).unwrap();
        let blue = right.node_pose(prop).unwrap();
        for (part, pose) in [(&retained.parts[0], red), (&retained.parts[1], blue)] {
            assert!(frustum.intersects_aabb(&part.posed_bounds(&pose, Mat4::IDENTITY).unwrap()));
        }
        for retained_geometry in [false, true] {
            for reverse_submission in [false, true] {
                clear_background(BLACK);
                telemetry::reset_renderer_diagnostics();
                let mut frame = renderer.begin(camera.position, -Vec3::Z).unwrap();
                frame.set_output(GltfColorOutput::Linear);
                let mut order = [(0, red), (1, blue)];
                if reverse_submission {
                    order.reverse();
                }
                for (index, pose) in order {
                    if retained_geometry {
                        frame
                            .submit_retained_part_posed(&retained.parts[index], &pose)
                            .unwrap();
                    } else {
                        frame.submit_part_posed(&cpu.parts[index], &pose).unwrap();
                    }
                }
                assert_eq!(frame.draw().blend_draws, 2);
                pixel(
                    &read(target),
                    32,
                    if red_front {
                        [128, 0, 64, 255]
                    } else {
                        [64, 0, 128, 255]
                    },
                    "hierarchy parent depths sort globally",
                );
                assert_no_uploads(retained_geometry);
            }
        }
    }
    macroquad::logging::info!("GLTF_HIERARCHY_FIXTURE_PASS");
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
    let mut renderer = GltfMaterialRenderer::new().unwrap();
    telemetry::set_renderer_diagnostics_enabled(true);

    for reflected_rest in [false, true] {
        let bytes = glb(reflected_rest);
        let scene = macroquad::gltf::GltfScene::from_bytes(&bytes).unwrap();
        let (cpu, poses) = GltfModel::from_bytes_with_pose(&bytes).unwrap();
        assert_eq!(cpu.parts.len(), 1);
        let part = &cpu.parts[0];
        let retained = RetainedGltfModel::from_model(cpu.clone(), false).unwrap();
        assert!(retained.cpu_model().is_none());
        let mut pose = poses.node(part.node).unwrap();
        assert_eq!(pose.node(), part.node);
        assert_eq!(pose.rest_world(), scene.nodes[part.node].world);
        close(
            pose.rest_world().transform_point3(Vec3::ZERO),
            vec3(3., 1.5, 0.1),
        );
        let spawn = scene.nodes[scene.spawns[0].node].world;

        // The rest pose is completely offscreen. Selecting it must not
        // implicitly recenter the world-baked geometry or move another node.
        clear_background(BLACK);
        let mut frame = renderer.begin(camera.position, -Vec3::Z).unwrap();
        frame.submit_part_posed(part, &pose).unwrap();
        assert_eq!(frame.draw().opaque_draws, 1);
        pixel(&read(&target), 16, [0, 0, 0, 255], "rest remains offscreen");

        for replacement_sign in [1., -1.] {
            // Replace the imported transform by the spawn's yaw and origin,
            // then apply a local nonuniform scale. Adding a world offset or
            // rotating baked vertices around world zero gives other pixels.
            let replacement = spawn * Mat4::from_scale(vec3(replacement_sign, 0.75, 0.5));
            pose.set_world(replacement).unwrap();
            let extent = vec3(0.3 * 3_f32.sqrt() * 0.5, 0.225, 0.15);
            let center = vec3(-0.5, 0., 0.);
            for bounds in [
                part.posed_bounds(&pose, Mat4::IDENTITY).unwrap(),
                retained.parts[0]
                    .posed_bounds(&pose, Mat4::IDENTITY)
                    .unwrap(),
            ] {
                close(bounds.min, center - extent);
                close(bounds.max, center + extent);
            }

            // Independent inverse-transpose result: local normal (1,2,3)
            // divided by (sign,.75,.5), then rotated 30 degrees about +Y.
            let x = replacement_sign * 3_f32.sqrt() * 0.5 + 3.;
            let y = 8. / 3.;
            let z = -replacement_sign * 0.5 + 3. * 3_f32.sqrt();
            let expected = ((z / (x * x + y * y + z * z).sqrt()) * 255.).round() as u8;
            let mut reference = None;
            for retained_geometry in [false, true] {
                for _ in 0..2 {
                    clear_background(BLACK);
                    telemetry::reset_renderer_diagnostics();
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
                    if retained_geometry {
                        let unrelated =
                            GltfNodePose::new(part.node + 1, pose.rest_world()).unwrap();
                        assert!(frame
                            .submit_retained_part_posed(&retained.parts[0], &unrelated)
                            .is_err());
                        frame
                            .submit_retained_part_posed(&retained.parts[0], &pose)
                            .unwrap();
                    } else {
                        let unrelated =
                            GltfNodePose::new(part.node + 1, pose.rest_world()).unwrap();
                        assert!(frame.submit_part_posed(part, &unrelated).is_err());
                        frame.submit_part_posed(part, &pose).unwrap();
                    }
                    assert_eq!(
                        frame.draw(),
                        GltfMaterialStats {
                            opaque_draws: 1,
                            ..Default::default()
                        }
                    );
                    let image = read(&target);
                    pixel(
                        &image,
                        16,
                        [expected, expected, expected, 255],
                        "replacement origin/yaw and lit normal",
                    );
                    pixel(
                        &image,
                        32,
                        [0, 0, 0, 255],
                        "replacement does not rotate around world origin",
                    );
                    pixel(&image, 48, [0, 0, 0, 255], "unselected space stays clear");
                    if let Some(reference) = &reference {
                        same_pixels(&image, reference);
                    } else {
                        reference = Some(image);
                    }
                    assert_no_uploads(retained_geometry);
                }
            }

            // The direct current-material helpers use the same replacement
            // without cloning vertices. Both helpers must restore the parent
            // matrix so the next draw still uses the caller's transform.
            let parent = Mat4::from_scale_rotation_translation(
                vec3(-1., 1., 1.),
                Quat::IDENTITY,
                Vec3::ZERO,
            );
            unsafe { get_internal_gl().quad_gl.push_model_matrix(parent) };
            let bounds = part.posed_bounds(&pose, parent).unwrap();
            close(bounds.min, vec3(0.5, 0., 0.) - extent);
            close(bounds.max, vec3(0.5, 0., 0.) + extent);
            for retained_geometry in [false, true] {
                clear_background(BLACK);
                telemetry::reset_renderer_diagnostics();
                gl_use_default_material();
                if retained_geometry {
                    retained.parts[0].draw_posed(&pose).unwrap();
                } else {
                    part.draw_posed(&pose).unwrap();
                }
                let image = read(&target);
                pixel(
                    &image,
                    16,
                    [0, 0, 0, 255],
                    "direct draw composes parent reflection",
                );
                pixel(
                    &image,
                    48,
                    [255, 255, 255, 255],
                    "direct posed draw uses replacement",
                );
                assert_no_uploads(retained_geometry);
            }
            unsafe { get_internal_gl().quad_gl.pop_model_matrix() };
        }
    }
    hierarchy(&mut renderer, &camera, &target);
    telemetry::set_renderer_diagnostics_enabled(false);
    set_default_camera();
    macroquad::logging::info!("GLTF_POSE_FIXTURE_PASS");
}
