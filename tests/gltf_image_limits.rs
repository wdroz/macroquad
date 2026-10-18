#![cfg(feature = "gltf")]

use macroquad::gltf::{GltfImageLimits, GltfScene};
use macroquad::gltf_draw::{GltfLoadOptions, GltfLoadSettings, GltfMeshLimits, GltfModel};
use serde_json::{json, Value};

fn limits(pixels: u64, bytes: u64) -> GltfImageLimits {
    GltfImageLimits {
        max_source_pixels: Some(pixels),
        max_decoded_bytes: Some(bytes),
    }
}

fn small_image(format: image::ImageOutputFormat) -> Vec<u8> {
    let mut result = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(2, 3, image::Rgb([17, 42, 89])))
        .write_to(&mut result, format)
        .unwrap();
    result.into_inner()
}

fn huge_png() -> Vec<u8> {
    let mut png = small_image(image::ImageOutputFormat::Png);
    png[16..20].copy_from_slice(&1_000_000u32.to_be_bytes());
    png[20..24].copy_from_slice(&1_000_000u32.to_be_bytes());
    // Deliberately keep a tiny original payload and stale CRC: a limit error
    // must win before a decoder checks the payload or allocates the image.
    png
}

fn fixture(images: &[(&[u8], &str)], edit: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut binary = vec![0; 60]; // Three positions and TEXCOORD_0 entries.
    let mut views = vec![
        json!({"buffer":0,"byteLength":36}),
        json!({"buffer":0,"byteOffset":36,"byteLength":24}),
    ];
    let mut image_json = Vec::new();
    let mut textures = Vec::new();
    let mut materials = Vec::new();
    let mut primitives = Vec::new();
    for (index, (bytes, mime)) in images.iter().enumerate() {
        binary.resize(binary.len().next_multiple_of(4), 0);
        views.push(json!({"buffer":0,"byteOffset":binary.len(),"byteLength":bytes.len()}));
        binary.extend_from_slice(bytes);
        image_json
            .push(json!({"name":format!("source-{index}"),"bufferView":index+2,"mimeType":mime}));
        textures.push(json!({"source":index}));
        materials.push(json!({"name":format!("paint-{index}"),"pbrMetallicRoughness":{"baseColorTexture":{"index":index}}}));
        primitives.push(json!({"attributes":{"POSITION":0,"TEXCOORD_0":1},"material":index}));
    }
    let mut document = json!({
        "asset":{"version":"2.0"}, "scene":0,
        "scenes":[{"nodes":[0]}], "nodes":[{"mesh":0}],
        "meshes":[{"primitives":primitives}], "materials":materials,
        "images":image_json,"textures":textures,
        "buffers":[{"byteLength":binary.len()}], "bufferViews":views,
        "accessors":[
            {"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[0,0,0]},
            {"bufferView":1,"componentType":5126,"count":3,"type":"VEC2"}
        ]
    });
    edit(&mut document);
    let mut json_bytes = serde_json::to_vec(&document).unwrap();
    json_bytes.resize(json_bytes.len().next_multiple_of(4), b' ');
    binary.resize(binary.len().next_multiple_of(4), 0);
    let mut glb = b"glTF".to_vec();
    glb.extend_from_slice(&2u32.to_le_bytes());
    glb.extend_from_slice(&((28 + json_bytes.len() + binary.len()) as u32).to_le_bytes());
    glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"JSON");
    glb.extend_from_slice(&json_bytes);
    glb.extend_from_slice(&(binary.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"BIN\0");
    glb.extend_from_slice(&binary);
    glb
}

fn draw_error(bytes: &[u8], image_limits: GltfImageLimits) -> String {
    // A call that reaches a GPU upload would panic outside a running app.
    let settings = GltfLoadOptions {
        mesh_limits: GltfMeshLimits::default(),
        max_texture_size: Some(1),
    }
    .with_image_limits(image_limits);
    match GltfModel::from_bytes_with_settings(bytes, settings) {
        Ok(_) => panic!("expected source image rejection"),
        Err(error) => error,
    }
}

#[test]
fn small_png_jpeg_webp_fit_exact_source_budgets() {
    for (format, mime) in [
        (image::ImageOutputFormat::Png, "image/png"),
        (image::ImageOutputFormat::Jpeg(80), "image/jpeg"),
        (image::ImageOutputFormat::WebP, "image/webp"),
    ] {
        let image = small_image(format);
        let bytes = fixture(&[(&image, mime)], |_| {});
        let scene = GltfScene::from_bytes_with_image_limits(&bytes, limits(6, 24)).unwrap();
        assert_eq!((scene.images[0].width, scene.images[0].height), (2, 3));
        assert_eq!(scene.images[0].pixels.len(), 24);
        assert_eq!(scene, GltfScene::from_bytes(&bytes).unwrap());
        assert_eq!(
            scene,
            GltfScene::from_bytes_with_image_limits(&bytes, GltfImageLimits::default()).unwrap()
        );
    }
}

#[test]
fn limits_are_independent_and_reject_before_post_decode_resizing() {
    let image = small_image(image::ImageOutputFormat::Png);
    let bytes = fixture(&[(&image, "image/png")], |_| {});
    for caps in [
        GltfImageLimits {
            max_source_pixels: Some(5),
            max_decoded_bytes: None,
        },
        GltfImageLimits {
            max_source_pixels: None,
            max_decoded_bytes: Some(23),
        },
        GltfImageLimits {
            max_source_pixels: Some(0),
            max_decoded_bytes: None,
        },
    ] {
        let error = draw_error(&bytes, caps);
        assert!(
            error.contains("image 0") && error.contains("source-0"),
            "{error}"
        );
        assert!(
            error.contains("material 0") && error.contains("paint-0"),
            "{error}"
        );
        assert!(GltfScene::from_bytes_with_image_limits(&bytes, caps).is_err());
    }
}

#[test]
fn all_draw_images_are_checked_before_the_first_upload_including_emissive() {
    let first = small_image(image::ImageOutputFormat::Png);
    let second = huge_png();
    for emissive in [false, true] {
        let bytes = fixture(&[(&first, "image/png"), (&second, "image/png")], |json| {
            if emissive {
                json["materials"][1]["pbrMetallicRoughness"] = json!({});
                json["materials"][1]["emissiveTexture"] = json!({"index":1});
            }
        });
        let error = draw_error(&bytes, limits(100, 400));
        assert!(
            error.contains("image 1") && error.contains("paint-1"),
            "{error}"
        );
        assert!(
            error.contains(if emissive { "emissive" } else { "base-color" }),
            "{error}"
        );
        assert!(error.contains("1000000"), "{error}");
    }
}

#[test]
fn cpu_preflight_checks_later_images_before_decoding_earlier_payloads() {
    let mut corrupt = small_image(image::ImageOutputFormat::Png);
    corrupt.truncate(33); // Full IHDR only: cannot successfully decode pixels.
    let huge = huge_png();
    let bytes = fixture(&[(&corrupt, "image/png"), (&huge, "image/png")], |_| {});
    let error = GltfScene::from_bytes_with_image_limits(&bytes, limits(100, 400))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("image 1") && error.contains("paint-1"),
        "{error}"
    );
    assert!(error.contains("1000000"), "{error}");
}

#[test]
fn rgba_estimate_overflow_is_rejected_by_both_public_constructors() {
    let mut image = huge_png();
    image[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
    image[20..24].copy_from_slice(&u32::MAX.to_be_bytes());
    let bytes = fixture(&[(&image, "image/png")], |_| {});
    let caps = GltfImageLimits {
        max_source_pixels: None,
        max_decoded_bytes: Some(u64::MAX),
    };
    for error in [
        draw_error(&bytes, caps),
        GltfScene::from_bytes_with_image_limits(&bytes, caps)
            .unwrap_err()
            .to_string(),
    ] {
        assert!(error.contains("overflows"), "{error}");
        assert!(
            error.contains("image 0") && error.contains("paint-0"),
            "{error}"
        );
    }
}

#[test]
fn unused_images_materials_scenes_and_unsupported_slots_are_ignored() {
    let huge = huge_png();
    let bytes = fixture(&[(&huge, "image/png")], |json| {
        json["meshes"][0]["primitives"][0]
            .as_object_mut()
            .unwrap()
            .remove("material");
        json["meshes"]
            .as_array_mut()
            .unwrap()
            .push(json!({"primitives":[{"attributes":{"POSITION":0},"material":0}]}));
        json["nodes"]
            .as_array_mut()
            .unwrap()
            .push(json!({"mesh":1}));
        json["scenes"]
            .as_array_mut()
            .unwrap()
            .push(json!({"nodes":[1]}));
        // Even an unreachable external URI must not fail preflight.
        json["images"][0] = json!({"uri":"unavailable.png"});
    });
    assert!(
        GltfScene::from_bytes_with_image_limits(&bytes, limits(1, 4))
            .unwrap()
            .images
            .is_empty()
    );
    assert_eq!(
        GltfModel::from_bytes_with_settings(
            &bytes,
            GltfLoadOptions::default().with_image_limits(limits(1, 4))
        )
        .unwrap()
        .stats
        .textures,
        0
    );

    let bytes = fixture(&[(&huge, "image/png")], |json| {
        json["materials"][0]["pbrMetallicRoughness"] = json!({});
        json["materials"][0]["normalTexture"] = json!({"index":0});
        json["materials"][0]["emissiveTexture"] = json!({"index":0});
    });
    // CPU scene intentionally consumes only base-color; draw also consumes emissive.
    assert!(
        GltfScene::from_bytes_with_image_limits(&bytes, limits(1, 4))
            .unwrap()
            .images
            .is_empty()
    );
    assert!(draw_error(&bytes, limits(1, 4)).contains("emissive"));
}

#[test]
fn public_options_literals_and_uncapped_wrapper_remain_compatible() {
    let options = GltfLoadOptions {
        mesh_limits: GltfMeshLimits::default(),
        max_texture_size: None,
    };
    assert_eq!(
        options
            .with_image_limits(GltfImageLimits::default())
            .image_limits,
        GltfImageLimits::default()
    );
    assert_eq!(
        GltfLoadSettings::default().image_limits,
        GltfImageLimits::default()
    );
}
