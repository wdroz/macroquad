#![cfg(feature = "gltf")]

use std::borrow::Cow;

use image::ImageEncoder;
use macroquad::{prelude::*, telemetry};
use serde_json::json;

fn png(color: [u8; 4]) -> Vec<u8> {
    let mut result = Vec::new();
    image::codecs::png::PngEncoder::new(&mut result)
        .write_image(&color.repeat(4), 2, 2, image::ColorType::Rgba8)
        .unwrap();
    result
}

// Generated entirely in memory: two images, an unused source mesh, shared and
// mirrored instances, and small chunk limits. No game assets or Blender.
fn fixture(corrupt_second_image: bool) -> Vec<u8> {
    let mut binary = Vec::new();
    for value in [
        -0.6f32, -0.6, 0., 0.6, -0.6, 0., 0.6, 0.6, 0., -0.6, 0.6, 0., 0., 0., 1., 0., 1., 1., 0.,
        1.,
    ] {
        binary.extend_from_slice(&value.to_le_bytes());
    }
    for index in [0u16, 1, 2, 0, 2, 3] {
        binary.extend_from_slice(&index.to_le_bytes());
    }
    let mut views = vec![
        json!({"buffer":0,"byteLength":48}),
        json!({"buffer":0,"byteOffset":48,"byteLength":32}),
        json!({"buffer":0,"byteOffset":80,"byteLength":12}),
    ];
    let first = png([210, 40, 20, 255]);
    let mut second = png([20, 180, 70, 255]);
    if corrupt_second_image {
        // A valid PNG header allows source-size preflight. Pixel decoding must
        // fail only after the first image was uploaded on an earlier poll.
        second.truncate(33);
    }
    for image in [first, second] {
        binary.resize(binary.len().next_multiple_of(4), 0);
        views.push(json!({"buffer":0,"byteOffset":binary.len(),"byteLength":image.len()}));
        binary.extend_from_slice(&image);
    }
    let primitive = |material| json!({"attributes":{"POSITION":0,"TEXCOORD_0":1},"indices":2,"material":material});
    let document = json!({
        "asset":{"version":"2.0"},"scene":0,
        "extensionsUsed":["KHR_materials_unlit","KHR_materials_emissive_strength"],
        "scenes":[{"nodes":[0]}],
        "nodes":[
            {"name":"root","children":[1,2,3]},
            {"name":"first","mesh":2,"translation":[-1.5,0,0],"extras":{"mq_role":"collider","mq_tag":"wall"}},
            {"name":"second","mesh":1,"extras":{"mq_role":"trigger","mq_shape":"sphere","mq_tag":"goal"}},
            {"name":"reflected","mesh":2,"translation":[1.5,0,0],"scale":[-1,1,1],"extras":{"mq_role":"spawn","mq_tag":"player"}}
        ],
        "meshes":[
            {"name":"unused","primitives":[primitive(0)]},
            {"name":"green","primitives":[primitive(1)]},
            {"name":"red","primitives":[primitive(0)]}
        ],
        "materials":[
            {"name":"red","pbrMetallicRoughness":{"baseColorTexture":{"index":0},"metallicFactor":0.2,"roughnessFactor":0.7},
             "emissiveTexture":{"index":0},"emissiveFactor":[0.1,0.2,0.3],"doubleSided":true,
             "extensions":{"KHR_materials_unlit":{},"KHR_materials_emissive_strength":{"emissiveStrength":2}}},
            {"name":"green","pbrMetallicRoughness":{"baseColorTexture":{"index":1}},"emissiveTexture":{"index":2},"alphaMode":"MASK","alphaCutoff":0.3}
        ],
        "samplers":[{"minFilter":9987,"magFilter":9728},{"minFilter":9729,"magFilter":9729,"wrapS":33071,"wrapT":33071}],
        "textures":[{"source":0,"sampler":0},{"source":1,"sampler":0},{"source":1,"sampler":1}],
        "images":[{"name":"red pixels","bufferView":3,"mimeType":"image/png"},{"name":"green pixels","bufferView":4,"mimeType":"image/png"}],
        "buffers":[{"byteLength":binary.len()}],"bufferViews":views,
        "accessors":[
            {"bufferView":0,"componentType":5126,"count":4,"type":"VEC3","min":[-0.6,-0.6,0],"max":[0.6,0.6,0]},
            {"bufferView":1,"componentType":5126,"count":4,"type":"VEC2"},
            {"bufferView":2,"componentType":5123,"count":6,"type":"SCALAR"}
        ]
    });
    gltf::binary::Glb {
        header: gltf::binary::Header {
            magic: *b"glTF",
            version: 2,
            length: 0,
        },
        json: Cow::Owned(serde_json::to_vec(&document).unwrap()),
        bin: Some(Cow::Owned(binary)),
    }
    .to_vec()
    .unwrap()
}

fn settings() -> GltfLoadSettings {
    GltfLoadOptions {
        mesh_limits: GltfMeshLimits {
            vertices: 3,
            indices: 3,
        },
        max_texture_size: Some(1),
    }
    .with_image_limits(GltfImageLimits {
        max_source_pixels: Some(4),
        max_decoded_bytes: Some(16),
    })
}

fn assert_same_model(actual: &GltfModel, expected: &GltfModel) {
    assert_eq!(actual.root_names, expected.root_names);
    assert_eq!(actual.bounds_min, expected.bounds_min);
    assert_eq!(actual.bounds_max, expected.bounds_max);
    assert_eq!(actual.stats.nodes, expected.stats.nodes);
    assert_eq!(actual.stats.meshes, expected.stats.meshes);
    assert_eq!(actual.stats.draw_calls, expected.stats.draw_calls);
    assert_eq!(actual.stats.triangles, expected.stats.triangles);
    assert_eq!(actual.stats.textures, expected.stats.textures);
    assert_eq!(actual.parts.len(), expected.parts.len());
    for (actual, expected) in actual.parts.iter().zip(&expected.parts) {
        assert_eq!(actual.node, expected.node);
        assert_eq!(actual.name, expected.name);
        assert_eq!(actual.path, expected.path);
        assert_eq!(actual.center, expected.center);
        assert_eq!(actual.bounds_min, expected.bounds_min);
        assert_eq!(actual.bounds_max, expected.bounds_max);
        assert_eq!(
            format!("{:?}", actual.surface),
            format!("{:?}", expected.surface)
        );
        assert_eq!(actual.mesh.indices, expected.mesh.indices);
        assert_eq!(actual.mesh.vertices.len(), expected.mesh.vertices.len());
        for (actual, expected) in actual.mesh.vertices.iter().zip(&expected.mesh.vertices) {
            assert_eq!(actual.position, expected.position);
            assert_eq!(actual.normal, expected.normal);
            assert_eq!(actual.uv, expected.uv);
            assert_eq!(actual.color, expected.color);
        }
        for (actual, expected) in [
            (&actual.mesh.texture, &expected.mesh.texture),
            (&actual.emissive_texture, &expected.emissive_texture),
        ] {
            match (actual, expected) {
                (Some(actual), Some(expected)) => {
                    assert_eq!(actual.size(), expected.size());
                    assert_eq!(
                        actual.get_texture_data().bytes,
                        expected.get_texture_data().bytes
                    );
                }
                (None, None) => {}
                _ => panic!("texture presence differs"),
            }
        }
    }
}

fn read_target(target: &RenderTarget) -> Image {
    unsafe { get_internal_gl().flush() };
    target.texture.get_texture_data()
}

fn assert_resources(textures: usize) {
    retire_unused_render_resources();
    assert_eq!(telemetry::textures_count(), textures);
    assert_eq!(telemetry::renderer_diagnostics().retained_mesh_count, 0);
}

fn assert_progress_step(before: GltfLoadProgress, after: GltfLoadProgress) {
    assert!(after.completed <= after.total, "{after:?}");
    if before.stage == after.stage {
        assert_eq!(before.total, after.total);
        assert!(after.completed >= before.completed);
        if matches!(
            after.stage,
            GltfLoadStage::LoadTextures
                | GltfLoadStage::ConvertMeshes
                | GltfLoadStage::ValidateMeshes
                | GltfLoadStage::UploadMeshes
        ) {
            assert!(
                after.completed - before.completed <= 1,
                "more than one unit: {before:?} -> {after:?}"
            );
        }
    } else {
        assert_eq!(
            before.completed, before.total,
            "stage ended early: {before:?}"
        );
        assert_eq!(
            after.completed, 0,
            "new stage must yield before work: {after:?}"
        );
    }
}

// A single native display per binary. Every construction poll is followed by
// a real frame so a loading screen can draw while textures and meshes accrue.
#[macroquad::test]
async fn pollable_loading_yields_matches_sync_and_releases_every_partial_state() {
    let bytes = fixture(false);
    let target = render_target(96, 64);
    let camera = Camera2D {
        zoom: vec2(1. / 3., 1. / 2.),
        render_target: Some(target.clone()),
        ..Default::default()
    };
    next_frame().await;
    telemetry::set_renderer_diagnostics_enabled(true);
    set_gltf_resource_diagnostics_enabled(true);
    retire_unused_render_resources();
    let baseline = telemetry::textures_count();
    let reference = GltfModel::from_bytes_with_settings(&bytes, settings()).unwrap();
    assert_eq!(reference.parts.len(), 6);
    assert_eq!(reference.stats.textures, 3);
    assert_eq!(reference.stats.meshes, 2);
    assert_eq!(reference.stats.nodes, 4);
    assert_eq!(reference.stats.triangles, 6);
    assert_eq!(reference.root_names, ["root"]);
    assert!((reference.bounds_min - vec3(-2.1, -0.6, 0.)).length() < 1.0e-6);
    assert!((reference.bounds_max - vec3(2.1, 0.6, 0.)).length() < 1.0e-6);
    for (index, part) in reference.parts.iter().enumerate() {
        assert_eq!(part.node, index / 2 + 1);
        assert_eq!(
            part.path,
            ["root/first", "root/second", "root/reflected"][index / 2]
        );
        assert_eq!(part.mesh.vertices.len(), 3);
        assert_eq!(part.mesh.indices.len(), 3);
        let vertices = &part.mesh.vertices;
        let indices = &part.mesh.indices;
        let [a, b, c] = [indices[0], indices[1], indices[2]].map(|i| vertices[i as usize].position);
        assert!(
            (b - a).cross(c - a).z > 0.,
            "mirrored winding must be corrected"
        );
        assert!(vertices
            .iter()
            .all(|vertex| (vertex.uv.x == 0. || vertex.uv.x == 1.)
                && (vertex.uv.y == 0. || vertex.uv.y == 1.)));
    }
    assert_eq!(reference.parts[0].surface.name.as_deref(), Some("red"));
    assert_eq!(reference.parts[0].surface.emissive, [0.2, 0.4, 0.6]);
    assert!(reference.parts[0].surface.unlit);
    assert!(reference.parts[0].surface.double_sided);
    assert_eq!(reference.parts[2].surface.alpha_mode, GltfAlphaMode::Mask);
    assert_eq!(reference.parts[2].surface.alpha_cutoff, 0.3);
    set_camera(&camera);
    clear_background(BLACK);
    reference.draw();
    let expected_pixels = read_target(&target);
    assert!(
        expected_pixels
            .bytes
            .chunks_exact(4)
            .filter(|pixel| pixel[..3] != [0, 0, 0])
            .count()
            > 800
    );
    for (x, expected) in [
        (24, [210, 40, 20, 255]),
        (48, [20, 180, 70, 255]),
        (72, [210, 40, 20, 255]),
    ] {
        let offset = (32 * expected_pixels.width as usize + x) * 4;
        assert_eq!(&expected_pixels.bytes[offset..offset + 4], &expected);
    }
    let reference_textures = telemetry::textures_count();

    let mut loader = GltfModelLoader::new_with_markers(&bytes, settings()).unwrap();
    let mut model_trace = vec![loader.progress()];
    assert_eq!(loader.progress().stage, GltfLoadStage::Parse);
    assert!(loader.take_model().is_none());
    assert!(loader.take_markers().is_none());
    while loader.progress().stage != GltfLoadStage::Complete {
        let before = loader.progress();
        let previous_textures = telemetry::textures_count();
        let progress = loader.poll().unwrap();
        if progress.stage != GltfLoadStage::Complete {
            assert!(loader.take_markers().is_none());
        }
        assert_progress_step(before, progress);
        let new_textures = telemetry::textures_count() - previous_textures;
        assert!(new_textures <= 1, "one poll uploaded {new_textures} images");
        if new_textures > 0 {
            assert_eq!(before.stage, GltfLoadStage::LoadTextures);
        }
        model_trace.push(progress);
        assert!(model_trace.len() < 100, "loader stopped progressing");
        set_camera(&camera);
        clear_background(BLACK);
        draw_rectangle(-3., -2., 6., 4., WHITE);
        let pixels = read_target(&target);
        assert_eq!(&pixels.bytes[0..4], &[255, 255, 255, 255]);
        next_frame().await;
    }
    assert_eq!(loader.poll().unwrap(), loader.progress());
    let model = loader.take_model().unwrap();
    assert!(loader.take_model().is_none());
    loader.cancel(); // A taken model no longer belongs to the loader.
    let markers = loader.take_markers().unwrap();
    assert!(loader.take_markers().is_none());
    let scene = GltfScene::from_bytes(&bytes).unwrap();
    assert_eq!(markers, GltfMarkers::from_scene(&scene));
    assert_eq!(markers.solid_colliders().count(), 1);
    assert_eq!(markers.trigger_colliders().count(), 1);
    assert_eq!(markers.marker_tag(markers.spawns[0].node), Some("player"));
    drop(scene);
    drop(loader);
    assert_same_model(&model, &reference);
    assert_eq!(model.resource_report().textures.len(), 3);
    assert_eq!(
        model.resource_report().textures[0]
            .source
            .as_ref()
            .unwrap()
            .width,
        2
    );
    assert_eq!(model.resource_report().textures[0].width, 1);
    assert_eq!(
        model.parts[0]
            .mesh
            .texture
            .as_ref()
            .unwrap()
            .raw_miniquad_id(),
        model.parts[4]
            .mesh
            .texture
            .as_ref()
            .unwrap()
            .raw_miniquad_id()
    );
    assert_ne!(
        model.parts[2]
            .mesh
            .texture
            .as_ref()
            .unwrap()
            .raw_miniquad_id(),
        model.parts[2]
            .emissive_texture
            .as_ref()
            .unwrap()
            .raw_miniquad_id(),
        "distinct samplers of the same source image must keep separate textures"
    );
    set_camera(&camera);
    clear_background(BLACK);
    model.draw();
    assert_eq!(read_target(&target).bytes, expected_pixels.bytes);

    // Retained conversion reuses the already parsed/baked model and textures.
    let mut retained_loader = RetainedGltfModelLoader::new(model, true);
    let mut retained_trace = vec![retained_loader.progress()];
    let before_retained_textures = telemetry::textures_count();
    while retained_loader.progress().stage != GltfLoadStage::Complete {
        let before = retained_loader.progress();
        telemetry::reset_renderer_diagnostics();
        let progress = retained_loader.poll().unwrap();
        assert_progress_step(before, progress);
        let uploads = telemetry::renderer_diagnostics()
            .counters
            .retained_buffer_uploads;
        assert!(uploads <= 2, "one poll uploaded {uploads} buffers");
        if uploads > 0 {
            assert_eq!(before.stage, GltfLoadStage::UploadMeshes);
            assert_eq!(uploads, 2);
        }
        assert_eq!(telemetry::textures_count(), before_retained_textures);
        retained_trace.push(progress);
        assert!(retained_trace.len() < 100);
        next_frame().await;
    }
    let retained = retained_loader.take_model().unwrap();
    assert_same_model(retained.cpu_model().unwrap(), &reference);
    assert!(retained_loader.take_model().is_none());
    retained_loader.cancel();
    drop(retained_loader);
    set_camera(&camera);
    clear_background(BLACK);
    retained.draw(Mat4::IDENTITY);
    assert_eq!(read_target(&target).bytes, expected_pixels.bytes);
    drop(retained);
    assert_resources(reference_textures);

    // Retained conversion rebuilds stale bounds after caller edits, and the
    // release option must not silently retain the CPU source.
    let mut edited = reference.clone();
    edited.parts[0].mesh.vertices[0].position.x = -4.;
    let mut loader = RetainedGltfModelLoader::new(edited, false);
    while loader.poll().unwrap().stage != GltfLoadStage::Complete {}
    let edited = loader.take_model().unwrap();
    assert!(edited.cpu_model().is_none());
    assert_eq!(edited.bounds_min.x, -4.);
    assert_eq!(edited.parts[0].bounds_min.x, -4.);
    drop(edited);
    drop(loader);
    assert_resources(reference_textures);

    // A bad later chunk fails during the validation stage, before any GPU
    // geometry upload. Even a still-live failed loader releases its textures.
    let mut invalid = GltfModel::from_bytes_with_settings(&bytes, settings()).unwrap();
    invalid.parts[1].mesh.indices[0] = u16::MAX;
    let mut loader = RetainedGltfModelLoader::new(invalid, false);
    telemetry::reset_renderer_diagnostics();
    let mut validated_first = false;
    while let Ok(progress) = loader.poll() {
        validated_first |=
            progress.stage == GltfLoadStage::ValidateMeshes && progress.completed > 0;
        assert_ne!(progress.stage, GltfLoadStage::Complete);
    }
    assert!(validated_first);
    assert_eq!(loader.progress().stage, GltfLoadStage::Failed);
    assert_eq!(
        telemetry::renderer_diagnostics()
            .counters
            .retained_buffer_uploads,
        0
    );
    assert!(loader.take_model().is_none());
    assert_resources(reference_textures);
    drop(loader);

    // Stop at every observed boundary, including halfway through each stage
    // and after completion while the output is still owned by the loader.
    for drop_only in [false, true] {
        for (stop, expected) in model_trace.iter().enumerate() {
            let mut loader = GltfModelLoader::new_with_markers(&bytes, settings()).unwrap();
            for _ in 0..stop {
                loader.poll().unwrap();
            }
            assert_eq!(loader.progress(), *expected);
            if !drop_only {
                loader.cancel();
                assert_eq!(loader.progress().stage, GltfLoadStage::Cancelled);
                assert!(loader.poll().is_err());
                assert!(loader.take_model().is_none());
                assert!(loader.take_markers().is_none());
                loader.cancel();
            }
            drop(loader);
            assert_resources(reference_textures);
        }
        for (stop, expected) in retained_trace.iter().enumerate() {
            let cpu = GltfModel::from_bytes_with_settings(&bytes, settings()).unwrap();
            let mut loader = RetainedGltfModelLoader::new(cpu, false);
            for _ in 0..stop {
                loader.poll().unwrap();
            }
            assert_eq!(loader.progress(), *expected);
            if !drop_only {
                loader.cancel();
                assert_eq!(loader.progress().stage, GltfLoadStage::Cancelled);
                assert!(loader.poll().is_err());
                assert!(loader.take_model().is_none());
                loader.cancel();
            }
            drop(loader);
            assert_resources(reference_textures);
        }
    }

    let invalid = fixture(true);
    let mut loader = GltfModelLoader::new_with_markers(&invalid, settings()).unwrap();
    let mut saw_first_texture = false;
    let error = loop {
        match loader.poll() {
            Ok(_) => {
                saw_first_texture |= telemetry::textures_count() > reference_textures;
                assert_ne!(loader.progress().stage, GltfLoadStage::Complete);
            }
            Err(error) => break error,
        }
    };
    assert!(
        saw_first_texture,
        "invalid second image must fail after a real upload"
    );
    assert!(
        error.contains("image 1") && error.contains("green"),
        "{error}"
    );
    assert_eq!(loader.progress().stage, GltfLoadStage::Failed);
    assert!(loader.poll().is_err());
    assert!(loader.take_model().is_none());
    assert!(loader.take_markers().is_none());
    assert_resources(reference_textures); // Even a still-live failed handle releases resources.
    drop(loader);

    // Conversion can fail after every texture has uploaded and an earlier
    // primitive has already built output chunks. The failed handle must drop
    // both those chunks' texture owners and its upload cache without drawing.
    let source = gltf::binary::Glb::from_slice(&bytes).unwrap();
    let mut document: serde_json::Value = serde_json::from_slice(&source.json).unwrap();
    document["materials"][1]["pbrMetallicRoughness"]["baseColorTexture"]["texCoord"] = json!(1);
    let invalid_uvs = gltf::binary::Glb {
        header: source.header,
        json: Cow::Owned(serde_json::to_vec(&document).unwrap()),
        bin: source.bin,
    }
    .to_vec()
    .unwrap();
    let mut loader = GltfModelLoader::new_with_markers(&invalid_uvs, settings()).unwrap();
    while loader.progress().stage != GltfLoadStage::ConvertMeshes {
        loader.poll().unwrap();
    }
    assert_eq!(telemetry::textures_count(), reference_textures + 3);
    assert_eq!(loader.poll().unwrap().completed, 1);
    let error = loader.poll().unwrap_err();
    assert!(
        error.contains("green") && error.contains("TEXCOORD_1"),
        "{error}"
    );
    assert_eq!(loader.progress().stage, GltfLoadStage::Failed);
    assert!(loader.take_model().is_none());
    assert!(loader.take_markers().is_none());
    assert_resources(reference_textures);
    loader.cancel();
    assert_eq!(loader.poll().unwrap_err(), error);
    assert_eq!(loader.progress().stage, GltfLoadStage::Failed);
    assert_resources(reference_textures);
    drop(loader);

    // A subsequent load must still render correctly after retiring the failed
    // load's resources, including on backends that may recycle texture names.
    let restarted = GltfModel::from_bytes_with_settings(&bytes, settings()).unwrap();
    set_camera(&camera);
    clear_background(BLACK);
    restarted.draw();
    assert_eq!(read_target(&target).bytes, expected_pixels.bytes);
    drop(restarted);
    assert_resources(reference_textures);
    drop(reference);
    assert_resources(baseline);
    // The still-live marker sidecar cannot keep any texture or geometry alive.
    assert_eq!(
        markers.marker_rotation(markers.spawns[0].node).unwrap() * Vec3::Y,
        Vec3::Y
    );
    drop(markers);
    assert_resources(baseline);
    set_gltf_resource_diagnostics_enabled(false);
    set_default_camera();
    next_frame().await;
}
