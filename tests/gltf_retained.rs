#![cfg(feature = "gltf")]

use macroquad::camera::Camera;
use macroquad::miniquad::{CullFace, FrontFaceOrder, UniformDesc, UniformType};
use macroquad::prelude::*;

fn fixture() -> GltfModel {
    GltfModel::from_bytes(include_bytes!("../examples/assets/textured_quad.glb")).unwrap()
}

fn read_target(target: &RenderTarget) -> Image {
    unsafe { get_internal_gl().flush() };
    target.texture.get_texture_data()
}

fn assert_same_pixels(actual: &Image, expected: &Image) {
    assert_eq!(actual.width, expected.width);
    assert_eq!(actual.height, expected.height);
    for (index, (actual, expected)) in actual.bytes.iter().zip(&expected.bytes).enumerate() {
        assert!(
            actual.abs_diff(*expected) <= 2,
            "pixel {} channel {}: expected {expected}, got {actual}",
            index / 4,
            index % 4,
        );
    }
}

fn colored_pixels(image: &Image) -> usize {
    image
        .bytes
        .chunks_exact(4)
        .filter(|rgba| rgba[..3] != [0, 0, 0])
        .count()
}

fn bake_transform(model: &GltfModel, transform: Mat4) -> GltfModel {
    let mut result = model.clone();
    let normal_matrix = transform.inverse().transpose();
    result.bounds_min = Vec3::splat(f32::INFINITY);
    result.bounds_max = Vec3::splat(f32::NEG_INFINITY);
    for part in &mut result.parts {
        part.bounds_min = Vec3::splat(f32::INFINITY);
        part.bounds_max = Vec3::splat(f32::NEG_INFINITY);
        for vertex in &mut part.mesh.vertices {
            vertex.position = transform.transform_point3(vertex.position);
            vertex.normal = normal_matrix
                .transform_vector3(vertex.normal.truncate())
                .normalize()
                .extend(vertex.normal.w);
            part.bounds_min = part.bounds_min.min(vertex.position);
            part.bounds_max = part.bounds_max.max(vertex.position);
        }
        if transform.determinant() < 0. {
            for triangle in part.mesh.indices.chunks_exact_mut(3) {
                triangle.swap(1, 2);
            }
        }
        part.center = (part.bounds_min + part.bounds_max) * 0.5;
        result.bounds_min = result.bounds_min.min(part.bounds_min);
        result.bounds_max = result.bounds_max.max(part.bounds_max);
    }
    result
}

fn normal_material(front_face_order: FrontFaceOrder) -> Material {
    load_material(
        ShaderSource::Glsl {
            vertex: r#"#version 100
                attribute vec3 position;
                attribute vec4 normal;
                uniform mat4 Model;
                uniform mat4 Projection;
                uniform mat4 NormalMatrix;
                varying lowp vec3 world_normal;
                void main() {
                    gl_Position = Projection * Model * vec4(position, 1.0);
                    world_normal = normalize((NormalMatrix * vec4(normal.xyz, 0.0)).xyz);
                }"#,
            fragment: r#"#version 100
                precision lowp float;
                varying lowp vec3 world_normal;
                void main() {
                    gl_FragColor = vec4(normalize(world_normal) * 0.5 + 0.5, 1.0);
                }"#,
        },
        MaterialParams {
            pipeline_params: PipelineParams {
                cull_face: CullFace::Back,
                front_face_order,
                ..Default::default()
            },
            uniforms: vec![UniformDesc::new("NormalMatrix", UniformType::Mat4)],
            ..Default::default()
        },
    )
    .unwrap()
}

// One windowed test per binary: miniquad keeps a process-wide native display.
#[macroquad::test]
async fn retained_gltf_matches_cpu_rendering_transforms_culling_and_lifetimes() {
    let target = render_target(96, 64);
    let camera = Camera2D {
        // Positive Y keeps the fixture's CCW winding in the render target.
        zoom: vec2(1. / 3., 1. / 2.),
        render_target: Some(target.clone()),
        ..Default::default()
    };
    set_camera(&camera);

    let mut source = fixture();
    assert_eq!(source.stats.textures, 1);
    assert_eq!(source.parts.len(), 1);
    source.parts[0].emissive_texture = source.parts[0].mesh.texture.clone();
    let base_texture = source.parts[0]
        .mesh
        .texture
        .as_ref()
        .unwrap()
        .raw_miniquad_id();
    let retained = RetainedGltfModel::from_model(source.clone(), false).unwrap();
    assert!(retained.cpu_model().is_none());
    assert_eq!(
        retained.parts[0]
            .mesh
            .texture
            .as_ref()
            .unwrap()
            .raw_miniquad_id(),
        base_texture
    );
    assert_eq!(
        retained.parts[0]
            .emissive_texture
            .as_ref()
            .unwrap()
            .raw_miniquad_id(),
        base_texture
    );
    assert_eq!(retained.parts[0].path, source.parts[0].path);
    assert_eq!(
        retained.parts[0].surface.base_color,
        source.parts[0].surface.base_color
    );
    assert_eq!(retained.root_names, source.root_names);

    clear_background(BLACK);
    source.draw();
    let original_image = read_target(&target);
    assert!(
        colored_pixels(&original_image) > 500,
        "fixture must actually draw"
    );
    clear_background(BLACK);
    retained.draw(Mat4::IDENTITY);
    assert_same_pixels(&read_target(&target), &original_image);

    // Explicit CPU retention remains available without changing the old model.
    let keep_cpu = RetainedGltfModel::from_model(source.clone(), true).unwrap();
    assert_eq!(
        keep_cpu.cpu_model().unwrap().parts[0].mesh.vertices.len(),
        4
    );
    drop(keep_cpu);

    // A non-axis normal exposes incorrect handling of nonuniform scale. The
    // custom material uses the adapter's documented normal/winding helper.
    let source_normal = vec3(1., 2., 3.).normalize();
    for vertex in &mut source.parts[0].mesh.vertices {
        vertex.normal = source_normal.extend(0.);
    }
    let normals = RetainedGltfModel::from_model(source.clone(), false).unwrap();
    let regular_material = normal_material(FrontFaceOrder::CounterClockwise);
    let mirrored_material = normal_material(FrontFaceOrder::Clockwise);
    let rotation = Quat::from_rotation_z(0.3) * Quat::from_rotation_y(0.2);
    for scale in [vec3(1.3, 0.65, 0.4), vec3(-1.3, 0.65, 0.4)] {
        let transform = Mat4::from_scale_rotation_translation(scale, rotation, Vec3::ZERO);
        let helper = GltfModelTransform::new(transform).unwrap();
        assert_eq!(helper.mirrored, scale.x < 0.);
        let expected_normal = (rotation * (source_normal / scale)).normalize();
        let actual_normal = helper
            .normal_matrix
            .transform_vector3(source_normal)
            .normalize();
        assert!((actual_normal - expected_normal).length() < 1.0e-5);

        let baked = bake_transform(&source, transform);
        clear_background(BLACK);
        gl_use_material(&regular_material);
        regular_material.set_uniform("NormalMatrix", Mat4::IDENTITY);
        baked.draw();
        let expected = read_target(&target);
        assert!(
            colored_pixels(&expected) > 500,
            "CPU reference must survive back-face culling"
        );

        if helper.mirrored {
            clear_background(BLACK);
            regular_material.set_uniform("NormalMatrix", helper.normal_matrix);
            normals.draw(helper.matrix);
            assert_eq!(
                colored_pixels(&read_target(&target)),
                0,
                "a reflected instance needs the opposite front-face order"
            );
        }

        clear_background(BLACK);
        let material = if helper.mirrored {
            &mirrored_material
        } else {
            &regular_material
        };
        gl_use_material(material);
        material.set_uniform("NormalMatrix", helper.normal_matrix);
        normals.draw(helper.matrix);
        let actual = read_target(&target);
        assert_same_pixels(&actual, &expected);
        let center = (32 * actual.width as usize + 48) * 4;
        for (value, normal) in actual.bytes[center..center + 3]
            .iter()
            .zip(expected_normal.to_array())
        {
            let expected = ((normal * 0.5 + 0.5) * 255.).round() as u8;
            assert!(
                value.abs_diff(expected) <= 2,
                "normal color: expected {expected}, got {value}"
            );
        }

        let bounds = normals.parts[0].world_bounds(transform).unwrap();
        assert!((bounds.min - baked.bounds_min).length() < 1.0e-5);
        assert!((bounds.max - baked.bounds_max).length() < 1.0e-5);
    }
    gl_use_default_material();
    drop(normals);
    drop(regular_material);
    drop(mirrored_material);

    // The source bounds are outside the camera. Model and parent transforms
    // move them into view; testing only the untransformed box would reject it.
    let shifted_source = bake_transform(&source, Mat4::from_translation(vec3(8., 0., 0.)));
    let shifted = RetainedGltfModel::from_model(shifted_source, false).unwrap();
    let frustum = Frustum::from_view_projection(camera.matrix()).unwrap();
    clear_background(BLACK);
    assert_eq!(shifted.draw_culled(Mat4::IDENTITY, &frustum), 0);
    assert_eq!(colored_pixels(&read_target(&target)), 0);
    assert_eq!(
        shifted.draw_culled(Mat4::from_translation(vec3(-8., 0., 0.)), &frustum),
        1
    );
    assert_same_pixels(&read_target(&target), &original_image);
    clear_background(BLACK);
    unsafe {
        get_internal_gl()
            .quad_gl
            .push_model_matrix(Mat4::from_translation(vec3(-4., 0., 0.)))
    };
    assert_eq!(
        shifted.draw_culled(Mat4::from_translation(vec3(-4., 0., 0.)), &frustum),
        1
    );
    unsafe { get_internal_gl().quad_gl.pop_model_matrix() };
    assert_same_pixels(&read_target(&target), &original_image);
    drop(shifted);

    // Both instances share textures and remain valid after every public model
    // handle (including the original CPU model) is dropped with draws queued.
    let clone = retained.clone();
    assert_eq!(
        clone.parts[0]
            .mesh
            .texture
            .as_ref()
            .unwrap()
            .raw_miniquad_id(),
        base_texture
    );
    let left =
        Mat4::from_scale_rotation_translation(Vec3::splat(0.6), Quat::IDENTITY, vec3(-1.4, 0., 0.));
    let right =
        Mat4::from_scale_rotation_translation(Vec3::splat(0.6), Quat::IDENTITY, vec3(1.4, 0., 0.));
    clear_background(BLACK);
    bake_transform(&source, left).draw();
    bake_transform(&source, right).draw();
    let expected = read_target(&target);
    assert!(colored_pixels(&expected) > 500);
    clear_background(BLACK);
    retained.draw(left);
    clone.draw(right);
    drop(source);
    drop(retained);
    drop(clone);
    assert_same_pixels(&read_target(&target), &expected);
    set_default_camera();
    next_frame().await;
}
