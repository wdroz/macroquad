#![cfg(feature = "gltf")]

use image::ImageEncoder;
use macroquad::gltf_draw::{GltfLoadSettings, GltfLoadStage, GltfModel, GltfModelLoader};
use macroquad::math::{vec2, Vec3};

// One windowed test per binary: miniquad keeps a process-wide native display.
#[macroquad::test]
async fn retained_maps_keep_distinct_uvs_shared_textures_and_mirrored_tangents() {
    let bytes = mirrored_fixture();
    let mut loader = GltfModelLoader::new(&bytes, GltfLoadSettings::default()).unwrap();
    assert!(loader.take_material_maps().is_none());
    while loader.poll().unwrap().stage != GltfLoadStage::Complete {}
    let model = loader.take_model().unwrap();
    let maps = loader.take_material_maps().unwrap();
    assert!(loader.take_material_maps().is_none());
    assert_eq!(model.stats.textures, 0);
    check_maps(&model, &maps);

    let assets =
        GltfModel::from_bytes_with_material_maps(&bytes, GltfLoadSettings::default()).unwrap();
    assert_eq!(assets.model.stats.textures, 0);
    assert_eq!(assets.material_maps.parts.len(), assets.model.parts.len());
    check_maps(&assets.model, &assets.material_maps);
    let turned = assets.material_maps.with_up_axis(Vec3::Z);
    assert!((turned.parts[0].tangents[0].truncate() - Vec3::new(-1.0, 0.0, 0.0)).length() < 1.0e-4);
    assert_eq!(turned.parts[0].tangents[0].w, -1.0);

    let parsed = gltf::binary::Glb::from_slice(&bytes).unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&parsed.json).unwrap();
    json["materials"][0]["normalTexture"]["extensions"]["KHR_texture_transform"]["texCoord"] =
        serde_json::json!(3);
    let broken = glb(json, parsed.bin.unwrap().as_ref());
    let error = GltfModel::from_bytes_with_material_maps(&broken, GltfLoadSettings::default())
        .map(|_| ())
        .unwrap_err();
    assert!(error.contains("mirrored"), "{error}");
    assert!(error.contains("normal map"), "{error}");
    assert!(error.contains("TEXCOORD_3"), "{error}");

    let limited = GltfModel::from_bytes_with_settings(
        &oversized_normal(),
        GltfLoadSettings {
            image_limits: macroquad::gltf::GltfImageLimits {
                max_source_pixels: Some(10),
                max_decoded_bytes: None,
            },
            ..Default::default()
        },
    )
    .map(|_| ())
    .unwrap_err();
    assert!(limited.contains("paint"), "{limited}");
    assert!(limited.contains("normal"), "{limited}");
    assert!(limited.contains("max_source_pixels"), "{limited}");
    assert!(!limited.contains("occlusion"), "{limited}");
}

fn check_maps(model: &GltfModel, maps: &macroquad::gltf_draw::GltfModelMaterialMaps) {
    assert_eq!(model.parts.len(), 2);
    assert_eq!(maps.parts.len(), 2);
    assert_eq!(model.parts[0].name, "panel");
    assert_eq!(model.parts[1].name, "mirror");
    let shared = maps.parts[0]
        .normal
        .as_ref()
        .unwrap()
        .texture
        .raw_miniquad_id();
    for (part, tangent, handedness) in [
        (&maps.parts[0], Vec3::new(-1.0, 0.0, 0.0), -1.0),
        (&maps.parts[1], Vec3::X, 1.0),
    ] {
        assert_eq!(part.material_name.as_deref(), Some("mirrored"));
        let normal = part.normal.as_ref().unwrap();
        let rough = part.metallic_roughness.as_ref().unwrap();
        assert_eq!(normal.texture.raw_miniquad_id(), shared);
        assert_eq!(rough.texture.raw_miniquad_id(), shared);
        assert_eq!(normal.strength, 2.0);
        assert_eq!(normal.tex_coord, 0);
        assert_eq!(rough.tex_coord, 1);
        assert_eq!(normal.uvs.len(), 3);
        assert_eq!(part.tangents.len(), 3);
        assert!(normal
            .uvs
            .iter()
            .any(|uv| (*uv - vec2(-1.0, 0.0)).length() < 1.0e-5));
        assert!(rough
            .uvs
            .iter()
            .any(|uv| (*uv - vec2(0.5, 0.25)).length() < 1.0e-5));
        assert!(rough
            .uvs
            .iter()
            .any(|uv| (*uv - vec2(1.5, 0.25)).length() < 1.0e-5));
        assert!(part.tangents.iter().all(|value| {
            (value.truncate() - tangent).length() < 1.0e-4 && value.w == handedness
        }));
    }
}

fn mirrored_fixture() -> Vec<u8> {
    let positions = f32s(&[0., 0., 0., 1., 0., 0., 0., 1., 0.]);
    let normals = f32s(&[0., 0., 1., 0., 0., 1., 0., 0., 1.]);
    let uv0 = f32s(&[0., 0., 1., 0., 0., 1.]);
    let uv1 = f32s(&[0., 0., 0., 1., 1., 0.]);
    let indices = [0u16, 1, 2]
        .iter()
        .flat_map(|index| index.to_le_bytes())
        .collect::<Vec<_>>();
    let image = png([10, 20, 30, 255]);
    let mut binary = Vec::new();
    let mut views = Vec::new();
    for chunk in [&positions, &normals, &uv0, &uv1, &indices, &image] {
        binary.resize(binary.len().next_multiple_of(4), 0);
        views.push(serde_json::json!({
            "buffer": 0,
            "byteOffset": binary.len(),
            "byteLength": chunk.len(),
        }));
        binary.extend_from_slice(chunk);
    }
    glb(
        serde_json::json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "extensionsUsed": ["KHR_texture_transform"],
            "scenes": [{"nodes": [0, 1]}],
            "nodes": [
                {"name": "panel", "mesh": 0},
                {"name": "mirror", "mesh": 0, "scale": [-1.0, 1.0, 1.0]}
            ],
            "meshes": [{"primitives": [{
                "attributes": {"POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2, "TEXCOORD_1": 3},
                "indices": 4,
                "material": 0
            }]}],
            "materials": [{
                "name": "mirrored",
                "normalTexture": {
                    "index": 0,
                    "scale": 2.0,
                    "extensions": {"KHR_texture_transform": {"scale": [-1.0, 1.0]}}
                },
                "pbrMetallicRoughness": {"metallicRoughnessTexture": {
                    "index": 0,
                    "extensions": {"KHR_texture_transform": {"offset": [0.5, 0.25], "texCoord": 1}}
                }}
            }],
            "textures": [{"source": 0}],
            "images": [{"bufferView": 5, "mimeType": "image/png"}],
            "buffers": [{"byteLength": binary.len()}],
            "bufferViews": views,
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 0]},
                {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3"},
                {"bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC2"},
                {"bufferView": 3, "componentType": 5126, "count": 3, "type": "VEC2"},
                {"bufferView": 4, "componentType": 5123, "count": 3, "type": "SCALAR"}
            ]
        }),
        &binary,
    )
}

fn oversized_normal() -> Vec<u8> {
    let mut image = png([1, 2, 3, 4]);
    image[16..20].copy_from_slice(&1_000u32.to_be_bytes());
    image[20..24].copy_from_slice(&1_000u32.to_be_bytes());
    let positions = f32s(&[0., 0., 0., 1., 0., 0., 0., 1., 0.]);
    let uvs = f32s(&[0., 0., 1., 0., 0., 1.]);
    let mut binary = positions;
    binary.resize(binary.len().next_multiple_of(4), 0);
    let uv_offset = binary.len();
    binary.extend_from_slice(&uvs);
    binary.resize(binary.len().next_multiple_of(4), 0);
    let image_offset = binary.len();
    binary.extend_from_slice(&image);
    glb(
        serde_json::json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"mesh": 0}],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "TEXCOORD_0": 1}, "material": 0}]}],
            "materials": [{
                "name": "paint",
                "normalTexture": {"index": 0},
                "occlusionTexture": {"index": 1}
            }],
            "textures": [{"source": 0}, {"source": 1}],
            "images": [
                {"bufferView": 2, "mimeType": "image/png"},
                {"uri": "missing-occlusion.png"}
            ],
            "buffers": [{"byteLength": binary.len()}],
            "bufferViews": [
                {"buffer": 0, "byteLength": 36},
                {"buffer": 0, "byteOffset": uv_offset, "byteLength": uvs.len()},
                {"buffer": 0, "byteOffset": image_offset, "byteLength": image.len()}
            ],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 0]},
                {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC2"}
            ]
        }),
        &binary,
    )
}

fn png(color: [u8; 4]) -> Vec<u8> {
    let mut result = Vec::new();
    image::codecs::png::PngEncoder::new(&mut result)
        .write_image(&color, 1, 1, image::ColorType::Rgba8)
        .unwrap();
    result
}

fn glb(json: serde_json::Value, binary: &[u8]) -> Vec<u8> {
    gltf::binary::Glb {
        header: gltf::binary::Header {
            magic: *b"glTF",
            version: 2,
            length: 0,
        },
        json: std::borrow::Cow::Owned(serde_json::to_vec(&json).unwrap()),
        bin: Some(std::borrow::Cow::Owned(binary.to_vec())),
    }
    .to_vec()
    .unwrap()
}

fn f32s(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}
