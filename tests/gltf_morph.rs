#![cfg(feature = "gltf")]

use macroquad::gltf::{GltfMorphData, GltfMorphPrimitive, GltfMorphTarget, GltfScene};
use macroquad::math::{vec3, Aabb, Mat4, Quat, Vec3};
use serde_json::{json, Value};

#[test]
fn tiny_face_zero_full_interpolated_and_extrapolated_weights_are_independent() {
    let bytes = face();
    let data = GltfMorphData::from_bytes(&bytes).unwrap();
    let original = data.clone();
    let primitive = &data.meshes[0].primitives[0];
    let base = primitive.positions.clone();
    for weights in [[0.0, 0.0], [1.0, 0.0], [0.5, 0.25], [-0.5, 2.0]] {
        let result = data.evaluate(0, 0, Some(&weights)).unwrap();
        for (index, &position) in result.positions.iter().enumerate() {
            let expected = base[index]
                + primitive.targets[0].position_deltas[index] * weights[0]
                + primitive.targets[1].position_deltas[index] * weights[1];
            assert_eq!(position, expected);
            assert!(result.bounds.unwrap().contains(position));
        }
        let expected_normal = (Vec3::Z + Vec3::Y * weights[0]).normalize();
        for normal in &result.normals {
            assert!((*normal - expected_normal).length() < 1.0e-6);
        }
    }
    let captured = data.evaluate(1, 0, None).unwrap();
    assert_eq!(captured, data.evaluate(0, 0, Some(&[1.0, 0.0])).unwrap());
    assert_eq!(
        data.evaluate(2, 0, None).unwrap(),
        primitive.evaluate(&[0.25, 0.5]).unwrap()
    );
    assert_eq!(
        data.evaluate(3, 0, None).unwrap(),
        primitive.evaluate(&[0.0, 0.0]).unwrap()
    );
    assert_ne!(captured, data.evaluate(0, 0, None).unwrap());
    assert_eq!(data, original);
    assert_eq!(data.meshes[0].weights, [0.25, 0.5]);
    assert_eq!(data.meshes[1].weights, [0.0, 0.0]);
}

#[test]
fn missing_deltas_normals_and_unaffected_primitives_stay_explicit() {
    let data = GltfMorphData::from_bytes(&face()).unwrap();
    let base = &data.meshes[0].primitives[0];
    let only_positions = base.evaluate(&[0.0, 3.0]).unwrap();
    assert_eq!(only_positions.normals, base.normals);
    let unaffected = &data.meshes[0].primitives[1];
    assert_eq!(unaffected.targets.len(), 2);
    assert_eq!(
        unaffected.evaluate(&[5.0, -9.0]).unwrap().positions,
        unaffected.positions
    );
    assert!(unaffected
        .targets
        .iter()
        .all(|target| target.position_deltas.is_empty()));

    let no_normals = edited(&face(), |json, _| {
        for mesh in json["meshes"].as_array_mut().unwrap() {
            for primitive in mesh["primitives"].as_array_mut().unwrap() {
                primitive["attributes"]
                    .as_object_mut()
                    .unwrap()
                    .remove("NORMAL");
                if let Some(targets) = primitive["targets"].as_array_mut() {
                    for target in targets {
                        target.as_object_mut().unwrap().remove("NORMAL");
                    }
                }
            }
        }
    });
    let data = GltfMorphData::from_bytes(&no_normals).unwrap();
    assert!(data.evaluate(1, 0, None).unwrap().normals.is_empty());
    let bad = edited(&face(), |json, _| {
        json["meshes"][0]["primitives"][0]["attributes"]
            .as_object_mut()
            .unwrap()
            .remove("NORMAL");
    });
    assert!(GltfMorphData::from_bytes(&bad).is_err());

    let tangent_only = edited(&face(), |json, binary| {
        let offset = binary.len();
        binary.extend(
            [1.0_f32, 0.0, 0.0, 1.0]
                .into_iter()
                .cycle()
                .take(12)
                .flat_map(f32::to_le_bytes),
        );
        json["bufferViews"]
            .as_array_mut()
            .unwrap()
            .push(json!({"buffer":0,"byteOffset":offset,"byteLength":48}));
        json["accessors"]
            .as_array_mut()
            .unwrap()
            .push(json!({"bufferView":5,"componentType":5126,"count":3,"type":"VEC4"}));
        json["buffers"][0]["byteLength"] = json!(binary.len());
        json["meshes"][0]["primitives"][0]["attributes"]["TANGENT"] = json!(5);
        json["meshes"][0]["primitives"][0]["targets"][0] = json!({"TANGENT": 2});
    });
    let tangent_only = GltfMorphData::from_bytes(&tangent_only).unwrap();
    assert_eq!(tangent_only.meshes[0].primitives[0].targets.len(), 2);
    assert_eq!(
        tangent_only.evaluate(1, 0, None).unwrap().positions,
        base.positions
    );
}

#[test]
fn deformed_bounds_follow_hierarchy_mirrors_shear_and_up_axes() {
    let bytes = edited(&face(), |json, _| {
        json["scenes"][0]["nodes"] = json!([2, 0]);
        json["nodes"][0]["children"] = json!([1, 3]);
        json["nodes"][0]["translation"] = json!([2.0, 3.0, 4.0]);
        json["nodes"][0]["scale"] = json!([-2.0, 1.0, 0.5]);
        json["nodes"][1]["rotation"] = json!(Quat::from_rotation_z(0.6).to_array());
        // First-used mesh differs from file order, as does node preorder.
        json["nodes"][2]["mesh"] = json!(1);
    });
    let data = GltfMorphData::from_bytes(&bytes).unwrap();
    let scene = GltfScene::from_bytes(&bytes).unwrap();
    assert_eq!(data.nodes.len(), scene.nodes.len());
    assert_eq!(data.meshes.len(), scene.meshes.len());
    for (node, source) in data.nodes.iter().zip(&scene.nodes) {
        assert_eq!(node.mesh, source.mesh);
    }
    for up in [Vec3::Y, Vec3::Z, vec3(1.0, 2.0, -3.0)] {
        let scene = scene.with_up_axis(up);
        for (index, node) in scene.nodes.iter().enumerate() {
            let result = data.evaluate(index, 0, Some(&[-0.5, 2.0])).unwrap();
            let bounds = result.transformed_bounds(node.world).unwrap().unwrap();
            let points = result
                .positions
                .iter()
                .map(|&position| node.world.transform_point3(position))
                .collect::<Vec<_>>();
            assert_eq!(bounds, Aabb::from_points(points.iter().copied()).unwrap());
            for triangle in points.chunks_exact(3) {
                // Interior samples remain inside under the same affine transform.
                let midpoint = (triangle[0] + triangle[1] + triangle[2]) / 3.0;
                assert!(bounds.contains(midpoint));
            }
        }
    }
    let result = data.evaluate(0, 0, None).unwrap();
    assert!(result.transformed_bounds(Mat4::ZERO).is_err());
    let singular = Mat4::from_scale(vec3(0.0, -1.0, 2.0));
    assert!(result.transformed_bounds(singular).unwrap().is_some());
    assert!(result
        .transformed_bounds(Mat4::perspective_rh_gl(1.0, 1.0, 0.1, 10.0))
        .is_err());
}

#[test]
fn malformed_accessor_counts_ranges_types_and_defaults_are_errors() {
    for (label, path, value) in [
        ("POSITION count", vec!["accessors", "2", "count"], json!(2)),
        ("NORMAL count", vec!["accessors", "3", "count"], json!(2)),
        (
            "base NORMAL count",
            vec!["accessors", "1", "count"],
            json!(2),
        ),
        ("wrong type", vec!["accessors", "2", "type"], json!("VEC2")),
        (
            "wrong component",
            vec!["accessors", "2", "componentType"],
            json!(5123),
        ),
        (
            "normalized",
            vec!["accessors", "2", "normalized"],
            json!(true),
        ),
        (
            "short view",
            vec!["bufferViews", "2", "byteLength"],
            json!(12),
        ),
        (
            "view past buffer",
            vec!["bufferViews", "2", "byteOffset"],
            json!(10000),
        ),
        (
            "misaligned offset",
            vec!["accessors", "2", "byteOffset"],
            json!(1),
        ),
        (
            "small stride",
            vec!["bufferViews", "2", "byteStride"],
            json!(8),
        ),
        ("zero count", vec!["accessors", "2", "count"], json!(0)),
        (
            "wrong mesh defaults",
            vec!["meshes", "0", "weights"],
            json!([1.0]),
        ),
        (
            "wrong node defaults",
            vec!["nodes", "0", "weights"],
            json!([1.0]),
        ),
    ] {
        let bytes = edited(&face(), |json, _| {
            let mut slot = json;
            for key in path {
                slot = if let Ok(index) = key.parse::<usize>() {
                    &mut slot[index]
                } else {
                    &mut slot[key]
                };
            }
            *slot = value;
        });
        assert!(
            GltfMorphData::from_bytes(&bytes).is_err(),
            "accepted {label}"
        );
    }
    let incompatible_targets = edited(&face(), |json, _| {
        json["meshes"][0]["primitives"][1]["targets"] = json!([{"POSITION": 2}]);
    });
    assert!(GltfMorphData::from_bytes(&incompatible_targets).is_err());
    let nonfinite = edited(&face(), |_, binary| {
        binary[72..76].copy_from_slice(&f32::INFINITY.to_le_bytes());
    });
    assert!(GltfMorphData::from_bytes(&nonfinite).is_err());
    let data = GltfMorphData::from_bytes(&face()).unwrap();
    for weights in [
        vec![],
        vec![1.0],
        vec![0.0, f32::NAN],
        vec![f32::INFINITY, 0.0],
    ] {
        assert!(data.evaluate(0, 0, Some(&weights)).is_err());
    }
    assert!(data.evaluate(100, 0, None).is_err());
    assert!(data.evaluate(0, 100, None).is_err());
}

#[test]
fn sparse_targets_expand_zero_base_and_reject_invalid_indices_and_ranges() {
    let bytes = sparse_face(&[0, 2]);
    let data = GltfMorphData::from_bytes(&bytes).unwrap();
    assert_eq!(
        data.meshes[0].primitives[0].targets[0].position_deltas,
        [Vec3::Y, Vec3::ZERO, Vec3::Y * 2.0]
    );
    let result = data.evaluate(1, 0, None).unwrap();
    assert_eq!(
        result.positions,
        [vec3(-1.0, 1.0, 0.0), Vec3::X, vec3(0.0, 3.0, 0.0)]
    );
    for indices in [&[0, 0][..], &[2, 0][..], &[0, 3][..]] {
        assert!(GltfMorphData::from_bytes(&sparse_face(indices)).is_err());
    }
    for broken in [
        edited(&bytes, |json, _| {
            json["accessors"][2]["sparse"]["count"] = json!(4);
        }),
        edited(&bytes, |json, _| {
            json["accessors"][2]["sparse"]["count"] = json!(0);
        }),
        edited(&bytes, |json, _| {
            json["bufferViews"][6]["byteLength"] = json!(12);
        }),
        edited(&bytes, |json, _| {
            json["bufferViews"][5]["byteLength"] = json!(1);
        }),
        edited(&bytes, |json, _| {
            json["bufferViews"][6]["byteStride"] = json!(12);
        }),
    ] {
        assert!(GltfMorphData::from_bytes(&broken).is_err());
    }
}

#[test]
fn manual_arrays_overflow_and_extreme_normals_are_checked_without_mutation() {
    let input = GltfMorphPrimitive {
        positions: vec![Vec3::ZERO],
        normals: vec![Vec3::Z],
        targets: vec![GltfMorphTarget {
            position_deltas: vec![Vec3::splat(f32::MAX)],
            normal_deltas: vec![Vec3::splat(f32::MAX)],
        }],
    };
    assert!(input.evaluate(&[2.0]).is_err());
    let result = input.evaluate(&[1.0]).unwrap();
    assert!((result.normals[0].length() - 1.0).abs() < 1.0e-6);
    assert!(result
        .transformed_bounds(Mat4::from_scale(Vec3::splat(2.0)))
        .is_err());
    let mut malformed = input.clone();
    malformed.targets[0].position_deltas.push(Vec3::ZERO);
    assert!(malformed.evaluate(&[0.0]).is_err());
    malformed = input.clone();
    malformed.normals.clear();
    assert!(malformed.evaluate(&[0.0]).is_err());
    let empty = GltfMorphPrimitive {
        positions: Vec::new(),
        normals: Vec::new(),
        targets: Vec::new(),
    }
    .evaluate(&[])
    .unwrap();
    assert!(empty.bounds.is_none());
    assert!(empty.transformed_bounds(Mat4::IDENTITY).unwrap().is_none());
}

#[test]
fn final_morph_validation_preserves_error_precedence_and_source_data() {
    let mut input = GltfMorphPrimitive {
        positions: vec![Vec3::ZERO; 3],
        normals: vec![Vec3::Z; 3],
        targets: vec![GltfMorphTarget {
            position_deltas: vec![Vec3::ZERO, Vec3::ZERO, Vec3::splat(f32::MAX)],
            normal_deltas: vec![Vec3::ZERO, Vec3::splat(f32::MAX), Vec3::ZERO],
        }],
    };
    let before = input.clone();
    assert_eq!(
        input.evaluate(&[2.]).unwrap_err().to_string(),
        "evaluated positions requires 3 finite VEC3 values, got 3"
    );
    assert_eq!(input, before);
    input.targets[0].position_deltas[2] = Vec3::ZERO;
    let before = input.clone();
    assert_eq!(
        input.evaluate(&[2.]).unwrap_err().to_string(),
        "evaluated normals requires 3 finite VEC3 values, got 3"
    );
    assert_eq!(input, before);
    // Even zero-weight target values must still be validated before evaluation.
    input.targets[0].normal_deltas[2] = Vec3::splat(f32::NAN);
    assert!(input
        .evaluate(&[0.])
        .unwrap_err()
        .to_string()
        .starts_with("NORMAL deltas"));
}

#[test]
fn morph_bounds_and_untouched_normals_preserve_exact_zero_bits() {
    let input = GltfMorphPrimitive {
        positions: vec![vec3(-0., 0., -0.), vec3(0., -0., 0.)],
        normals: vec![vec3(-0., 0., 2.), vec3(0., -0., 0.)],
        targets: vec![GltfMorphTarget::default()],
    };
    let expected_bounds = Aabb::from_points(input.positions.iter().copied()).unwrap();
    for weight in [0., -0., 2.] {
        let result = input.evaluate(&[weight]).unwrap();
        for (actual, expected) in result.normals.iter().zip(&input.normals) {
            assert_eq!(
                actual.to_array().map(f32::to_bits),
                expected.to_array().map(f32::to_bits)
            );
        }
        let bounds = result.bounds.unwrap();
        assert_eq!(
            bounds.min.to_array().map(f32::to_bits),
            expected_bounds.min.to_array().map(f32::to_bits)
        );
        assert_eq!(
            bounds.max.to_array().map(f32::to_bits),
            expected_bounds.max.to_array().map(f32::to_bits)
        );
    }
}

#[test]
fn selected_graph_validation_and_image_independence_match_cpu_contract() {
    let image = edited(&face(), |json, _| {
        json["images"] = json!([{"uri":"external-face.png"}]);
        json["textures"] = json!([{"source":0}]);
        json["materials"] = json!([{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}]);
        json["meshes"][0]["primitives"][0]["material"] = json!(0);
    });
    assert!(GltfMorphData::from_bytes(&image).is_ok());
    assert!(GltfScene::from_bytes(&image).is_err());
    let repeated = edited(&face(), |json, _| {
        json["scenes"][0]["nodes"] = json!([0, 0]);
    });
    assert!(GltfMorphData::from_bytes(&repeated).is_err());
    let cyclic = edited(&face(), |json, _| {
        json["nodes"][0]["children"] = json!([0]);
    });
    assert!(GltfMorphData::from_bytes(&cyclic).is_err());
    let unused = edited(&face(), |json, _| {
        json["meshes"].as_array_mut().unwrap().push(json!({"primitives":[{"attributes":{"POSITION":0},"targets":[{"POSITION":1},{"POSITION":1},{"POSITION":1}]}],"weights":[1.0]}));
    });
    assert!(GltfMorphData::from_bytes(&unused).is_ok());
    assert!(GltfMorphData::from_bytes(br#"{"asset":{"version":"2.0"}}"#)
        .unwrap()
        .nodes
        .is_empty());
    assert!(GltfMorphData::from_bytes(b"invalid").is_err());
}

fn face() -> Vec<u8> {
    let arrays = [
        [-1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
        [0.0, 0.5, 0.0, 0.0, 0.5, 0.0, 0.0, 0.25, 1.0],
        [0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0],
        [0.25, 0.0, 0.0, 0.25, 0.0, 0.0, 0.25, 0.0, 0.0],
    ];
    let binary = arrays
        .into_iter()
        .flatten()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    let primitive = json!({"attributes":{"POSITION":0,"NORMAL":1},"targets":[{"POSITION":2,"NORMAL":3},{"POSITION":4}]});
    let unaffected = json!({"attributes":{"POSITION":0,"NORMAL":1}});
    let mut json = json!({
        "asset":{"version":"2.0"}, "scenes":[{"nodes":[0,1,2,3]}],
        "nodes":[{"mesh":0,"weights":[0.0,0.0]},{"mesh":0,"weights":[1.0,0.0]},{"mesh":0},{"mesh":1}],
        "meshes":[{"primitives":[primitive.clone(),unaffected],"weights":[0.25,0.5]},{"primitives":[primitive]}],
        "buffers":[{"byteLength":binary.len()}],
        "bufferViews":(0..5).map(|index| json!({"buffer":0,"byteOffset":index*36,"byteLength":36})).collect::<Vec<_>>(),
        "accessors":(0..5).map(|index| json!({"bufferView":index,"componentType":5126,"count":3,"type":"VEC3"})).collect::<Vec<_>>()
    });
    json["accessors"][0]["min"] = json!([-1.0, 0.0, 0.0]);
    json["accessors"][0]["max"] = json!([1.0, 1.0, 0.0]);
    glb(json, &binary)
}

fn sparse_face(indices: &[u8]) -> Vec<u8> {
    edited(&face(), |json, binary| {
        let index_offset = binary.len();
        binary.extend_from_slice(indices);
        binary.resize(binary.len().next_multiple_of(4), 0);
        let value_offset = binary.len();
        binary.extend(
            [0.0_f32, 1.0, 0.0, 0.0, 2.0, 0.0]
                .into_iter()
                .flat_map(f32::to_le_bytes),
        );
        json["bufferViews"].as_array_mut().unwrap().extend([
            json!({"buffer":0,"byteOffset":index_offset,"byteLength":indices.len()}),
            json!({"buffer":0,"byteOffset":value_offset,"byteLength":24}),
        ]);
        json["accessors"][2] = json!({"componentType":5126,"count":3,"type":"VEC3","sparse":{
            "count":2,"indices":{"bufferView":5,"componentType":5121},"values":{"bufferView":6}
        }});
        json["buffers"][0]["byteLength"] = json!(binary.len());
    })
}

fn edited(bytes: &[u8], edit: impl FnOnce(&mut Value, &mut Vec<u8>)) -> Vec<u8> {
    let parsed = gltf::binary::Glb::from_slice(bytes).unwrap();
    let mut json = serde_json::from_slice(&parsed.json).unwrap();
    let mut binary = parsed.bin.unwrap().into_owned();
    edit(&mut json, &mut binary);
    glb(json, &binary)
}

fn glb(json: Value, binary: &[u8]) -> Vec<u8> {
    let mut json = serde_json::to_vec(&json).unwrap();
    json.resize(json.len().next_multiple_of(4), b' ');
    let mut binary = binary.to_vec();
    binary.resize(binary.len().next_multiple_of(4), 0);
    let mut out = Vec::new();
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2_u32.to_le_bytes());
    out.extend_from_slice(&((12 + 8 + json.len() + 8 + binary.len()) as u32).to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend(json);
    out.extend_from_slice(&(binary.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend(binary);
    out
}
