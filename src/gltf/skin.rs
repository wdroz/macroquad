//! CPU skeletal skinning, indexed like the scene and animation sidecars.

use std::{borrow::Cow, collections::HashMap};

use glam::{Mat4, Vec3};

use super::{load_buffers, GltfError};

/// One skin's scene-local joint indices and corresponding inverse bind matrices.
#[derive(Clone, Debug, PartialEq)]
pub struct GltfSkin {
    pub joints: Vec<usize>,
    pub inverse_bind_matrices: Vec<Mat4>,
}

/// Up to eight joint influences per vertex. Missing second sets are zero padded.
/// Weights are normalized at import, including quantized normalized integers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GltfSkinPrimitive {
    pub joints: Vec<[u16; 8]>,
    pub weights: Vec<[f32; 8]>,
}

/// Skin inputs for the default scene, without image decoding or GPU allocation.
/// Node and mesh indices match `GltfScene` and `GltfAnimationData` loaded from
/// the same bytes. Existing static/morph loaders retain their existing behavior.
///
/// ```
/// use macroquad::gltf::GltfSkinData;
/// let skin = GltfSkinData::from_bytes(br#"{"asset":{"version":"2.0"}}"#).unwrap();
/// assert!(skin.skins.is_empty());
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GltfSkinData {
    /// Skin index for each selected node, or None for an unskinned node.
    pub nodes: Vec<Option<usize>>,
    /// Selected skins, in first-use order.
    pub skins: Vec<GltfSkin>,
    /// Skin attributes for every selected mesh's triangle primitives.
    pub meshes: Vec<Vec<GltfSkinPrimitive>>,
}

/// An independent deformation in the skinned mesh node's local coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct GltfSkinEvaluation {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
}

impl GltfSkinData {
    /// Reads GLB or embedded-buffer glTF, JOINTS/WEIGHTS sets 0 and 1 and inverse
    /// bind matrices. Rejects missing pairs, bad counts/ranges, invalid weights,
    /// out-of-scene joints and additional influence sets explicitly.
    /// JOINTS are unnormalized UBYTE/USHORT VEC4; WEIGHTS are unnormalized FLOAT
    /// or normalized UBYTE/USHORT VEC4. Inverse binds are unnormalized FLOAT MAT4.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, GltfError> {
        let parse = |e| GltfError::new(format!("glTF skin document: {e}"));
        let (document, binary) = if bytes.starts_with(b"glTF") {
            let glb = gltf::binary::Glb::from_slice(bytes).map_err(parse)?;
            let parsed = gltf::Gltf::from_slice(&glb.json).map_err(parse)?;
            (parsed.document, glb.bin.map(|v| v.into_owned()))
        } else {
            (gltf::Gltf::from_slice(bytes).map_err(parse)?.document, None)
        };
        let buffers = load_buffers(&document, binary.as_deref())?;
        let mut ordered = Vec::new();
        let mut node_map = HashMap::new();
        let mut pending = document
            .default_scene()
            .or_else(|| document.scenes().next())
            .map(|scene| scene.nodes().collect::<Vec<_>>())
            .unwrap_or_default();
        pending.reverse();
        while let Some(node) = pending.pop() {
            if node_map.insert(node.index(), ordered.len()).is_some() {
                return Err(GltfError::new("glTF skin scene repeats a node"));
            }
            pending.extend(node.children().collect::<Vec<_>>().into_iter().rev());
            ordered.push(node);
        }
        let mut data = Self::default();
        let mut mesh_map = HashMap::new();
        let mut skin_map = HashMap::new();
        for node in &ordered {
            let skin_index = if let Some(skin) = node.skin() {
                let index = if let Some(&index) = skin_map.get(&skin.index()) {
                    index
                } else {
                    let joints = skin
                        .joints()
                        .map(|joint| {
                            node_map.get(&joint.index()).copied().ok_or_else(|| {
                                GltfError::new("glTF skin joint is outside the selected scene")
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    if joints.is_empty() {
                        return Err(GltfError::new("glTF skin has no joints"));
                    }
                    if let Some(accessor) = skin.inverse_bind_matrices() {
                        if accessor.data_type() != gltf::accessor::DataType::F32
                            || accessor.dimensions() != gltf::accessor::Dimensions::Mat4
                            || accessor.normalized()
                        {
                            return Err(GltfError::new(
                                "glTF inverse bind matrices require unnormalized FLOAT MAT4",
                            ));
                        }
                        check_accessor(&accessor, &buffers)?;
                        if accessor.count() != joints.len() {
                            return Err(GltfError::new(
                                "glTF inverse bind matrix count does not match joints",
                            ));
                        }
                    }
                    let inverse_bind_matrices = skin
                        .reader(|buffer| Some(buffers[buffer.index()].as_ref()))
                        .read_inverse_bind_matrices()
                        .map(|values| values.map(|m| Mat4::from_cols_array_2d(&m)).collect())
                        .unwrap_or_else(|| vec![Mat4::IDENTITY; joints.len()]);
                    if inverse_bind_matrices
                        .iter()
                        .any(|m| !affine(*m) || !m.inverse().is_finite())
                    {
                        return Err(GltfError::new(
                            "glTF inverse bind matrices must be finite invertible affine matrices",
                        ));
                    }
                    let index = data.skins.len();
                    data.skins.push(GltfSkin {
                        joints,
                        inverse_bind_matrices,
                    });
                    skin_map.insert(skin.index(), index);
                    index
                };
                Some(index)
            } else {
                None
            };
            data.nodes.push(skin_index);
            if let Some(mesh) = node.mesh() {
                let mesh_index = if let Some(&index) = mesh_map.get(&mesh.index()) {
                    index
                } else {
                    let mut primitives = Vec::new();
                    for primitive in mesh
                        .primitives()
                        .filter(|p| p.mode() == gltf::mesh::Mode::Triangles)
                    {
                        let count = primitive
                            .get(&gltf::Semantic::Positions)
                            .map_or(0, |v| v.count());
                        for (semantic, accessor) in primitive.attributes() {
                            if let gltf::Semantic::Joints(set) | gltf::Semantic::Weights(set) =
                                semantic
                            {
                                if set > 1 {
                                    return Err(GltfError::new(
                                        "glTF skin supports JOINTS/WEIGHTS sets 0 and 1",
                                    ));
                                }
                                // The glTF readers assume semantic formats and can panic
                                // on unsupported types; document validation does not
                                // enforce these attribute-specific requirements.
                                use gltf::accessor::{DataType, Dimensions};
                                let joints = matches!(semantic, gltf::Semantic::Joints(_));
                                let valid_type = match accessor.data_type() {
                                    DataType::U8 | DataType::U16 => accessor.normalized() != joints,
                                    DataType::F32 => !joints && !accessor.normalized(),
                                    _ => false,
                                };
                                if accessor.dimensions() != Dimensions::Vec4 || !valid_type {
                                    return Err(GltfError::new(if joints {
                                        "glTF JOINTS require unnormalized UBYTE/USHORT VEC4"
                                    } else {
                                        "glTF WEIGHTS require FLOAT or normalized UBYTE/USHORT VEC4"
                                    }));
                                }
                                check_accessor(&accessor, &buffers)?;
                                if accessor.count() != count {
                                    return Err(GltfError::new(
                                        "glTF skin attribute count differs from POSITION",
                                    ));
                                }
                            }
                        }
                        let reader =
                            primitive.reader(|buffer| Some(buffers[buffer.index()].as_ref()));
                        let mut input = GltfSkinPrimitive::default();
                        for set in 0..=1 {
                            match (reader.read_joints(set), reader.read_weights(set)) {
                                (None, None) => {}
                                (Some(joints), Some(weights)) => {
                                    if set == 1 && input.joints.is_empty() {
                                        return Err(GltfError::new(
                                            "glTF skin second influence set requires first set",
                                        ));
                                    }
                                    if input.joints.is_empty() {
                                        input.joints.resize(count, [0; 8]);
                                        input.weights.resize(count, [0.; 8]);
                                    }
                                    for (index, (joints, weights)) in
                                        joints.into_u16().zip(weights.into_f32()).enumerate()
                                    {
                                        input.joints[index][set as usize * 4..set as usize * 4 + 4]
                                            .copy_from_slice(&joints);
                                        input.weights[index]
                                            [set as usize * 4..set as usize * 4 + 4]
                                            .copy_from_slice(&weights);
                                    }
                                }
                                _ => {
                                    return Err(GltfError::new(
                                        "glTF skin JOINTS/WEIGHTS attributes must be paired",
                                    ))
                                }
                            }
                        }
                        for weights in &mut input.weights {
                            let sum: f32 = weights.iter().sum();
                            if weights.iter().any(|v| !v.is_finite() || *v < 0.)
                                || !sum.is_finite()
                                || sum <= 0.
                            {
                                return Err(GltfError::new("glTF skin vertex weights must be nonnegative with positive finite sum"));
                            }
                            for weight in weights {
                                *weight /= sum;
                            }
                        }
                        primitives.push(input);
                    }
                    let index = data.meshes.len();
                    data.meshes.push(primitives);
                    mesh_map.insert(mesh.index(), index);
                    index
                };
                if let Some(skin) = skin_index {
                    for primitive in &data.meshes[mesh_index] {
                        if primitive.joints.is_empty()
                            || primitive
                                .joints
                                .iter()
                                .flatten()
                                .any(|&j| j as usize >= data.skins[skin].joints.len())
                        {
                            return Err(GltfError::new("glTF skinned mesh is missing influences or references an invalid joint"));
                        }
                    }
                }
            } else if skin_index.is_some() {
                return Err(GltfError::new("glTF skinned node requires a mesh"));
            }
        }
        Ok(data)
    }

    /// Builds mesh-local matrices `inverse(mesh_world) * joint_world * inverse_bind`.
    /// Apply the mesh node's world matrix after deforming, exactly once. Worlds
    /// must correspond to this scene's current animation pose.
    pub fn joint_matrices(&self, node: usize, worlds: &[Mat4]) -> Result<Vec<Mat4>, GltfError> {
        if worlds.len() != self.nodes.len() {
            return Err(GltfError::new("glTF skin pose node count mismatch"));
        }
        let skin = self
            .nodes
            .get(node)
            .copied()
            .flatten()
            .and_then(|i| self.skins.get(i))
            .ok_or_else(|| GltfError::new("glTF node has no matching skin"))?;
        let inverse_mesh = worlds[node].inverse();
        if !affine(worlds[node]) || !inverse_mesh.is_finite() {
            return Err(GltfError::new(
                "glTF skin mesh world must be invertible affine",
            ));
        }
        skin.joints
            .iter()
            .zip(&skin.inverse_bind_matrices)
            .map(|(&joint, &bind)| {
                let matrix = worlds
                    .get(joint)
                    .copied()
                    .map(|world| inverse_mesh * world * bind)
                    .ok_or_else(|| GltfError::new("glTF skin joint index out of range"))?;
                if !affine(matrix) || !matrix.inverse().is_finite() {
                    return Err(GltfError::new(
                        "glTF skin joint transform must be finite invertible affine",
                    ));
                }
                Ok(matrix)
            })
            .collect()
    }
}

impl GltfSkinPrimitive {
    /// Linear blend skinning, including inverse-transpose joint normals. Source
    /// arrays and other instances stay unchanged. Positions may already include
    /// morph deltas; glTF applies morphing before skinning.
    pub fn deform(
        &self,
        positions: &[Vec3],
        normals: &[Vec3],
        matrices: &[Mat4],
    ) -> Result<GltfSkinEvaluation, GltfError> {
        let invalid = || {
            GltfError::new("glTF skin deformation has invalid arrays, indices, weights or matrices")
        };
        if self.joints.len() != positions.len()
            || self.weights.len() != positions.len()
            || !normals.is_empty() && normals.len() != positions.len()
            || positions.iter().chain(normals).any(|v| !v.is_finite())
            || self
                .joints
                .iter()
                .flatten()
                .any(|&j| j as usize >= matrices.len())
            || self.weights.iter().any(|weights| {
                weights.iter().any(|w| !w.is_finite() || *w < 0.)
                    || (weights.iter().sum::<f32>() - 1.).abs() > 0.001
            })
        {
            return Err(invalid());
        }
        // Validate every matrix, even unused joints and position-only calls.
        // Reuse the inverse for normals instead of computing it a second time.
        let mut normal_matrices = Vec::with_capacity(if normals.is_empty() {
            0
        } else {
            matrices.len()
        });
        for &matrix in matrices {
            if !affine(matrix) {
                return Err(invalid());
            }
            let inverse = matrix.inverse();
            if !inverse.is_finite() {
                return Err(invalid());
            }
            if !normals.is_empty() {
                normal_matrices.push(inverse.transpose());
            }
        }
        if normals.is_empty() {
            self.deform_vertices::<false>(positions, normals, matrices, &normal_matrices)
        } else {
            self.deform_vertices::<true>(positions, normals, matrices, &normal_matrices)
        }
    }

    // Specialize the influence loop once per call so position-only skinning
    // does not carry a normal branch through every joint influence.
    fn deform_vertices<const WITH_NORMALS: bool>(
        &self,
        positions: &[Vec3],
        normals: &[Vec3],
        matrices: &[Mat4],
        normal_matrices: &[Mat4],
    ) -> Result<GltfSkinEvaluation, GltfError> {
        let mut result = GltfSkinEvaluation {
            positions: Vec::with_capacity(positions.len()),
            normals: Vec::with_capacity(normals.len()),
        };
        for (index, ((&source_position, joints), weights)) in positions
            .iter()
            .zip(&self.joints)
            .zip(&self.weights)
            .enumerate()
        {
            let source_normal = if WITH_NORMALS {
                normals[index]
            } else {
                Vec3::ZERO
            };
            let mut position = Vec3::ZERO;
            let mut normal = Vec3::ZERO;
            for (&joint, &weight) in joints.iter().zip(weights) {
                if weight == 0. {
                    continue;
                }
                position += matrices[joint as usize].transform_point3(source_position) * weight;
                if WITH_NORMALS {
                    normal +=
                        normal_matrices[joint as usize].transform_vector3(source_normal) * weight;
                }
            }
            if !position.is_finite() || !normal.is_finite() {
                return Err(GltfError::new("glTF skin deformation overflow"));
            }
            result.positions.push(position);
            if WITH_NORMALS {
                result.normals.push(normal.normalize_or_zero());
            }
        }
        Ok(result)
    }
}

fn affine(matrix: Mat4) -> bool {
    matrix.is_finite()
        && matrix.x_axis.w.abs() < 0.0001
        && matrix.y_axis.w.abs() < 0.0001
        && matrix.z_axis.w.abs() < 0.0001
        && (matrix.w_axis.w - 1.).abs() < 0.0001
}

fn check_accessor(
    accessor: &gltf::Accessor<'_>,
    buffers: &[Cow<'_, [u8]>],
) -> Result<(), GltfError> {
    use super::morph::elements;
    if accessor.count() == 0 {
        return Err(GltfError::new("glTF skin accessor is empty"));
    }
    let size = accessor.size();
    let alignment = accessor.data_type().size();
    if let Some(view) = accessor.view() {
        elements(
            &view,
            accessor.offset(),
            accessor.count(),
            size,
            alignment,
            buffers,
            "glTF skin",
        )?;
    }
    if let Some(sparse) = accessor.sparse() {
        let indices = sparse.indices();
        let index_size = indices.index_type().size();
        let bytes = elements(
            &indices.view(),
            indices.offset(),
            sparse.count(),
            index_size,
            index_size,
            buffers,
            "glTF skin sparse indices",
        )?;
        let stride = indices.view().stride().unwrap_or(index_size);
        let mut previous = None;
        for i in 0..sparse.count() {
            let bytes = &bytes[i * stride..i * stride + index_size];
            let index = match index_size {
                1 => bytes[0] as usize,
                2 => u16::from_le_bytes(bytes.try_into().unwrap()) as usize,
                _ => u32::from_le_bytes(bytes.try_into().unwrap()) as usize,
            };
            if index >= accessor.count() || previous.is_some_and(|p| p >= index) {
                return Err(GltfError::new(
                    "glTF skin sparse indices out of range or unordered",
                ));
            }
            previous = Some(index);
        }
        let values = sparse.values();
        elements(
            &values.view(),
            values.offset(),
            sparse.count(),
            size,
            alignment,
            buffers,
            "glTF skin sparse values",
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skin_fixture(joint_type: u32, weight_type: u32) -> (serde_json::Value, Vec<u8>) {
        use serde_json::json;
        let positions = [0_f32, 0., 0., 1., 0., 0., 0., 1., 0.]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>();
        let joints = vec![0; if joint_type == 5121 { 12 } else { 24 }];
        let weights = (0..3)
            .flat_map(|_| match weight_type {
                5121 => vec![255, 0, 0, 0],
                5123 => [65535_u16, 0, 0, 0]
                    .into_iter()
                    .flat_map(u16::to_le_bytes)
                    .collect(),
                _ => [1_f32, 0., 0., 0.]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect(),
            })
            .collect::<Vec<_>>();
        let binds = Mat4::IDENTITY
            .to_cols_array()
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>();
        let mut binary = Vec::new();
        let mut views = Vec::new();
        for bytes in [positions, joints, weights, binds] {
            views.push(json!({"buffer": 0, "byteOffset": binary.len(), "byteLength": bytes.len()}));
            binary.extend(bytes);
        }
        let document = json!({
            "asset": {"version": "2.0"},
            "buffers": [{"byteLength": binary.len()}],
            "bufferViews": views,
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "type": "VEC3", "count": 3,
                 "min": [0, 0, 0], "max": [1, 1, 0]},
                {"bufferView": 1, "componentType": joint_type, "type": "VEC4", "count": 3},
                {"bufferView": 2, "componentType": weight_type, "type": "VEC4", "count": 3,
                 "normalized": weight_type != 5126},
                {"bufferView": 3, "componentType": 5126, "type": "MAT4", "count": 1}
            ],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "JOINTS_0": 1, "WEIGHTS_0": 2}}]}],
            "skins": [{"joints": [1], "inverseBindMatrices": 3}],
            "nodes": [{"mesh": 0, "skin": 0}, {}],
            "scenes": [{"nodes": [0, 1]}], "scene": 0
        });
        (document, binary)
    }

    fn fixture_glb(document: &serde_json::Value, binary: &[u8]) -> Vec<u8> {
        gltf::binary::Glb {
            header: gltf::binary::Header {
                magic: *b"glTF",
                version: 2,
                length: 0,
            },
            json: serde_json::to_vec(document).unwrap().into(),
            bin: Some(binary.into()),
        }
        .to_vec()
        .unwrap()
    }

    #[test]
    fn imports_supported_joint_and_weight_formats() {
        for joint_type in [5121, 5123] {
            for weight_type in [5121, 5123, 5126] {
                let (document, binary) = skin_fixture(joint_type, weight_type);
                let data = GltfSkinData::from_bytes(&fixture_glb(&document, &binary)).unwrap();
                assert_eq!(data.skins[0].inverse_bind_matrices, [Mat4::IDENTITY]);
                assert_eq!(data.meshes[0][0].joints, vec![[0; 8]; 3]);
                assert_eq!(
                    data.meshes[0][0].weights,
                    vec![[1., 0., 0., 0., 0., 0., 0., 0.]; 3]
                );
            }
        }
    }

    #[test]
    fn rejects_invalid_accessor_formats_before_calling_readers() {
        use serde_json::json;
        let (document, binary) = skin_fixture(5123, 5126);
        for (accessor, field, value, context) in [
            (1, "componentType", json!(5126), "JOINTS"),
            (1, "type", json!("SCALAR"), "JOINTS"),
            (1, "normalized", json!(true), "JOINTS"),
            (2, "componentType", json!(5125), "WEIGHTS"),
            (2, "componentType", json!(5121), "WEIGHTS"),
            (2, "componentType", json!(5123), "WEIGHTS"),
            (2, "type", json!("VEC3"), "WEIGHTS"),
            (2, "normalized", json!(true), "WEIGHTS"),
            (3, "componentType", json!(5123), "inverse bind"),
            (3, "type", json!("VEC4"), "inverse bind"),
            (3, "normalized", json!(true), "inverse bind"),
        ] {
            let mut invalid = document.clone();
            invalid["accessors"][accessor][field] = value;
            // Keep enough bytes for FLOAT JOINTS so byte-range validation alone
            // would succeed and the unchecked reader would reach unreachable!().
            invalid["bufferViews"][1]["byteLength"] = json!(48);
            let error = GltfSkinData::from_bytes(&fixture_glb(&invalid, &binary)).unwrap_err();
            assert!(error.to_string().contains(context), "{error}");
        }
    }

    #[test]
    fn mesh_world_is_not_applied_twice_and_instances_are_independent() {
        let skin = GltfSkinData {
            nodes: vec![Some(0), None],
            skins: vec![GltfSkin {
                joints: vec![1],
                inverse_bind_matrices: vec![Mat4::from_translation(-Vec3::Y)],
            }],
            meshes: vec![],
        };
        let first = skin
            .joint_matrices(
                0,
                &[
                    Mat4::from_translation(Vec3::X * 3.),
                    Mat4::from_translation(Vec3::Y * 2.),
                ],
            )
            .unwrap();
        let second = skin
            .joint_matrices(0, &[Mat4::IDENTITY, Mat4::from_translation(Vec3::Y)])
            .unwrap();
        let primitive = GltfSkinPrimitive {
            joints: vec![[0; 8]],
            weights: vec![[1., 0., 0., 0., 0., 0., 0., 0.]],
        };
        let a = primitive.deform(&[Vec3::ZERO], &[Vec3::Z], &first).unwrap();
        let b = primitive
            .deform(&[Vec3::ZERO], &[Vec3::Z], &second)
            .unwrap();
        assert!(
            (Mat4::from_translation(Vec3::X * 3.).transform_point3(a.positions[0]) - Vec3::Y)
                .length()
                < 0.0001
        );
        assert_eq!(b.positions[0], Vec3::ZERO);
        assert_eq!(a.normals[0], Vec3::Z);
    }
    #[test]
    fn weights_blend_joint_motion_and_normals_handle_nonuniform_scale() {
        let primitive = GltfSkinPrimitive {
            joints: vec![[0, 1, 0, 0, 0, 0, 0, 0]],
            weights: vec![[0.25, 0.75, 0., 0., 0., 0., 0., 0.]],
        };
        let matrices = [Mat4::IDENTITY, Mat4::from_translation(Vec3::Y * 4.)];
        assert_eq!(
            primitive
                .deform(&[Vec3::X], &[], &matrices)
                .unwrap()
                .positions[0],
            Vec3::X + Vec3::Y * 3.
        );
        let scaled = primitive
            .deform(
                &[Vec3::ZERO],
                &[Vec3::new(1., 1., 0.).normalize()],
                &[Mat4::from_scale(Vec3::new(2., 1., 1.)); 2],
            )
            .unwrap();
        assert!((scaled.normals[0] - Vec3::new(0.5, 1., 0.).normalize()).length() < 0.0001);
    }
    #[test]
    fn rejects_malformed_skin_inputs_without_panicking() {
        assert!(GltfSkinData::from_bytes(b"bad").is_err());
        let data = GltfSkinData::from_bytes(br#"{"asset":{"version":"2.0"}}"#).unwrap();
        assert!(data.joint_matrices(0, &[]).is_err());
        let primitive = GltfSkinPrimitive {
            joints: vec![[5; 8]],
            weights: vec![[1., 0., 0., 0., 0., 0., 0., 0.]],
        };
        assert!(primitive
            .deform(&[Vec3::ZERO], &[], &[Mat4::IDENTITY])
            .is_err());
    }
}
