#![cfg(feature = "gltf")]

use std::{borrow::Cow, mem::size_of};

use image::ImageEncoder;
use macroquad::prelude::*;

// Replace only the tiny library fixture's image and sampler: geometry remains
// four vertices / six u16 indices. No game assets or external tools are needed.
fn fixture(width: u32, height: u32) -> Vec<u8> {
    let original =
        gltf::binary::Glb::from_slice(include_bytes!("../examples/assets/textured_quad.glb"))
            .unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&original.json).unwrap();
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            &vec![255; (width * height * 4) as usize],
            width,
            height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    let view = json["images"][0]["bufferView"].as_u64().unwrap() as usize;
    let offset = json["bufferViews"][view]["byteOffset"].as_u64().unwrap() as usize;
    let mut binary = original.bin.unwrap()[..offset].to_vec();
    binary.extend_from_slice(&png);
    json["bufferViews"][view]["byteLength"] = serde_json::json!(png.len());
    json["buffers"][0]["byteLength"] = serde_json::json!(binary.len());
    json["images"][0]["name"] = serde_json::json!("diagnostic image");
    json["samplers"] = serde_json::json!([{ "minFilter": 9987, "magFilter": 9729 }]);
    json["textures"][0]["sampler"] = serde_json::json!(0);
    // The base-color and emissive slots deliberately reference one allocation.
    json["materials"][0]["emissiveTexture"] = serde_json::json!({ "index": 0 });
    gltf::binary::Glb {
        header: original.header,
        json: Cow::Owned(serde_json::to_vec(&json).unwrap()),
        bin: Some(Cow::Owned(binary)),
    }
    .to_vec()
    .unwrap()
}

// One native display per integration-test binary.
#[macroquad::test]
async fn model_reports_deduplicate_and_do_not_keep_dropped_resources_alive() {
    next_frame().await;
    let baseline = macroquad::telemetry::textures_count();
    let bytes = fixture(8, 4);
    let geometry_bytes = (4 * size_of::<Vertex>() + 6 * size_of::<u16>()) as u64;

    // Source metadata is opt-in, while allocation estimates always work.
    let untracked = GltfModel::from_bytes(&bytes).unwrap();
    let untracked_report = untracked.resource_report();
    assert_eq!(untracked_report.estimated_gpu_texture_bytes, 172);
    assert!(untracked_report.textures[0].source.is_none());
    drop(untracked);
    next_frame().await;
    assert_eq!(macroquad::telemetry::textures_count(), baseline);

    set_gltf_resource_diagnostics_enabled(true);
    let mut source = GltfModel::from_bytes(&bytes).unwrap();
    source.parts[0].mesh.vertices.reserve(17);
    source.parts[0].mesh.indices.reserve(19);
    let expected_capacity = (source.parts[0].mesh.vertices.capacity() * size_of::<Vertex>()
        + source.parts[0].mesh.indices.capacity() * size_of::<u16>())
        as u64;
    let report = source.resource_report();
    assert_eq!(report.cpu_geometry_bytes, geometry_bytes);
    assert_eq!(report.cpu_geometry_capacity_bytes, expected_capacity);
    assert!(expected_capacity > geometry_bytes);
    assert_eq!(report.gpu_buffer_count, 0);
    assert_eq!(report.estimated_gpu_buffer_bytes, 0);
    assert_eq!(report.textures.len(), 1);
    assert_eq!(report.textures[0].mip_levels, 4);
    assert_eq!(report.textures[0].estimated_gpu_bytes, 172);
    assert_eq!(report.textures[0].source.as_ref().unwrap().width, 8);
    assert_eq!(report.textures[0].source.as_ref().unwrap().height, 4);
    assert_eq!(report.textures[0].source.as_ref().unwrap().image_index, 0);
    assert_eq!(
        report.textures[0].source.as_ref().unwrap().name.as_deref(),
        Some("diagnostic image")
    );
    assert_eq!(
        GltfResourceReport::collect(&[&source, &source], &[]),
        report
    );

    // A CPU clone copies geometry, but both material slots still share one GPU image.
    let source_clone = source.clone();
    let cpu_clones = GltfResourceReport::collect(&[&source, &source_clone], &[]);
    assert_eq!(cpu_clones.cpu_geometry_bytes, geometry_bytes * 2);
    assert_eq!(cpu_clones.estimated_gpu_texture_bytes, 172);
    drop(source_clone);

    let retained = RetainedGltfModel::from_model(source, true).unwrap();
    let mut instance = retained.clone();
    let combined =
        GltfResourceReport::collect(&[retained.cpu_model().unwrap()], &[&retained, &instance]);
    assert_eq!(combined.cpu_geometry_bytes, geometry_bytes);
    assert_eq!(combined.cpu_geometry_capacity_bytes, expected_capacity);
    assert_eq!(combined.gpu_buffer_count, 2);
    assert_eq!(combined.estimated_gpu_buffer_bytes, geometry_bytes);
    assert_eq!(combined.estimated_gpu_texture_bytes, 172);

    // Changing the filter disables mip sampling, not allocated mip storage.
    retained.parts[0]
        .mesh
        .texture
        .as_ref()
        .unwrap()
        .set_filter(FilterMode::Nearest);
    assert_eq!(retained.resource_report().estimated_gpu_texture_bytes, 172);

    // Current overrides and the CPU model's original texture are both owned.
    instance.parts[0].mesh.texture = Some(Texture2D::from_rgba8(2, 2, &[255; 16]));
    instance.parts[0].emissive_texture = None;
    let overridden = instance.resource_report();
    assert_eq!(overridden.textures.len(), 2);
    assert_eq!(overridden.estimated_gpu_texture_bytes, 188);
    assert!(overridden.textures[1].source.is_none());
    drop(instance);
    drop(retained);
    next_frame().await;
    assert_eq!(macroquad::telemetry::textures_count(), baseline);

    // The upload cap does not change the original encoded image dimensions.
    let capped = GltfModel::from_bytes_with_options(
        &bytes,
        GltfLoadOptions {
            max_texture_size: Some(4),
            ..Default::default()
        },
    )
    .unwrap();
    let capped_report = capped.resource_report();
    assert_eq!(
        (
            capped_report.textures[0].width,
            capped_report.textures[0].height
        ),
        (4, 2)
    );
    assert_eq!(capped_report.textures[0].mip_levels, 3);
    assert_eq!(capped_report.estimated_gpu_texture_bytes, 44);
    assert_eq!(capped_report.textures[0].source.as_ref().unwrap().width, 8);
    set_gltf_resource_diagnostics_enabled(false);
    assert!(capped.resource_report().textures[0].source.is_none());
    set_gltf_resource_diagnostics_enabled(true);
    assert!(capped.resource_report().textures[0].source.is_none());
    drop(capped);
    next_frame().await;

    // Keep snapshots while repeatedly loading/drawing/dropping. Their metadata
    // and the registry's weak handles must not keep any GPU resource alive.
    let mut snapshots = Vec::new();
    for (width, height) in [(8, 4), (4, 2), (8, 4), (4, 2), (8, 4), (4, 2)] {
        let model = RetainedGltfModel::from_model(
            GltfModel::from_bytes(&fixture(width, height)).unwrap(),
            false,
        )
        .unwrap();
        let clone = model.clone();
        let snapshot = GltfResourceReport::collect(&[], &[&model, &clone]);
        assert_eq!(snapshot.cpu_geometry_bytes, 0);
        assert_eq!(snapshot.cpu_geometry_capacity_bytes, 0);
        assert_eq!(snapshot.gpu_buffer_count, 2);
        assert_eq!(snapshot.textures.len(), 1);
        assert_eq!(snapshot.textures[0].source.as_ref().unwrap().width, width);
        snapshots.push(snapshot);
        model.draw(Mat4::IDENTITY);
        clone.draw(Mat4::from_translation(Vec3::X));
        drop(model);
        drop(clone);
        next_frame().await;
        next_frame().await;
        assert_eq!(macroquad::telemetry::textures_count(), baseline);
        assert_eq!(
            GltfResourceReport::collect(&[], &[]),
            GltfResourceReport::default()
        );
    }
    assert_eq!(snapshots.len(), 6);
    set_gltf_resource_diagnostics_enabled(false);
}
