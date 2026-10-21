//! Independent CPU-evaluated morph instances backed by immutable GPU snapshots.

use std::sync::Arc;

use glam::Vec3;

use super::{
    GltfLoadSettings, GltfLoadStage, GltfModel, GltfModelLoader, GltfModelPose, RetainedGltfModel,
};
use crate::{gltf::GltfMorphData, math::Aabb, models::RetainedMesh};

pub(super) struct MorphChunk {
    pub node: usize,
    pub mesh: usize,
    pub primitive: usize,
    pub sources: Vec<u32>,
}

pub(super) struct MorphLoadData {
    pub data: GltfMorphData,
    pub indices: Vec<Vec<Vec<u32>>>,
    pub chunks: Vec<MorphChunk>,
}

struct MorphRest {
    input: MorphLoadData,
    model: GltfModel,
    pose: GltfModelPose,
}

/// Shared immutable rest geometry, morph inputs, textures and default buffers.
///
/// Loading opts in explicitly; ordinary [`GltfModel`] loading/drawing is
/// unchanged. Node-authored default weights are evaluated before the initial
/// upload. [`Self::instantiate`] creates independent weights without copying the
/// rest arrays or uploading again. Instances initially share immutable default
/// buffers; a changed node receives replacement buffers owned by that instance.
/// This is CPU evaluation, not shader morphing or hardware instancing.
///
/// Rest CPU geometry and POSITION/NORMAL deltas stay alive while this source or
/// any instance exists. Textures are shared. Resource reports for the exposed
/// retained model do not include these additional CPU arrays. Normal-map tangent
/// sidecars are not deformed; use the ordinary/custom normal-lighting path or
/// [`super::GltfMaterialFrame::submit_retained`], not stale shaded sidecars.
/// Clip playback uses the separate [`crate::gltf::GltfAnimationData`] sidecar;
/// skinning is not included.
///
/// ```no_run
/// use macroquad::prelude::*;
/// # fn example(bytes: &[u8], node: usize) -> Result<(), String> {
/// let source = GltfMorphModel::from_bytes(bytes)?;
/// let mut smile = source.instantiate();
/// let neutral = source.instantiate();
/// smile.set_weights(node, &[1.0])?;
/// smile.model().draw(Mat4::IDENTITY);
/// neutral.model().draw(Mat4::from_translation(Vec3::X * 3.));
/// # Ok(()) }
/// ```
#[derive(Clone)]
pub struct GltfMorphModel {
    rest: Arc<MorphRest>,
    defaults: RetainedGltfModel,
}

impl GltfMorphModel {
    /// Loads a binary GLB with default settings and evaluates default weights.
    ///
    /// Requires the render thread and an active context for nonempty geometry.
    /// Parsing, morph validation and conversion precede geometry uploads.
    /// Embedded textures follow [`GltfModel`] loading rules and are shared.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn load(bytes: &[u8]) -> Result<(), String> {
    /// let source = GltfMorphModel::from_bytes(bytes)?;
    /// let instance = source.instantiate();
    /// instance.model().draw(Mat4::IDENTITY);
    /// # Ok(()) }
    /// ```
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        Self::from_bytes_with_settings(bytes, GltfLoadSettings::default())
    }

    /// Loads with existing texture, source-image and mesh chunk limits.
    ///
    /// For staged texture/conversion work, use
    /// [`GltfModelLoader::new_with_morph`] and
    /// [`GltfModelLoader::take_morph_model`]. Final morph evaluation and retained
    /// uploads are synchronous; neither interface promises a frame-time budget.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn load(bytes: &[u8]) -> Result<GltfMorphModel, String> {
    /// GltfMorphModel::from_bytes_with_settings(bytes, GltfLoadSettings::default())
    /// # }
    /// ```
    pub fn from_bytes_with_settings(
        bytes: &[u8],
        settings: GltfLoadSettings,
    ) -> Result<Self, String> {
        let mut loader = GltfModelLoader::new_with_morph(bytes, settings)?;
        while loader.poll()?.stage != GltfLoadStage::Complete {}
        loader
            .take_morph_model()?
            .ok_or_else(|| "glTF morph source was already taken".into())
    }

    pub(super) fn from_loaded(
        model: GltfModel,
        pose: GltfModelPose,
        input: MorphLoadData,
    ) -> Result<Self, String> {
        if model.parts.len() != input.chunks.len() {
            return Err("glTF morph chunk correspondence was lost".into());
        }
        let rest = Arc::new(MorphRest { input, model, pose });
        let mut initial = rest.model.clone();
        // Evaluate the whole source before any retained buffer allocation, so
        // invalid defaults/overflow cannot leave a partially built instance.
        for (node, data) in rest.input.data.nodes.iter().enumerate() {
            if !data.weights.is_empty() {
                for (part, mesh) in evaluated_chunks(&rest, node, &data.weights)? {
                    initial.parts[part].mesh = mesh;
                    update_cpu_bounds(&mut initial.parts[part])?;
                }
            }
        }
        let mut defaults =
            RetainedGltfModel::from_model(initial, false).map_err(|e| e.to_string())?;
        // Aabb::center uses (min + max) / 2, which can overflow even when
        // both finite endpoints describe ordinary representable geometry.
        for part in &mut defaults.parts {
            part.center = part.bounds_min * 0.5 + part.bounds_max * 0.5;
        }
        Ok(Self { rest, defaults })
    }

    /// Borrows the shared CPU morph data, indexed by scene-local node IDs.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn inspect(source: &GltfMorphModel) {
    /// println!("{} nodes", source.data().nodes.len());
    /// # }
    /// ```
    pub fn data(&self) -> &GltfMorphData {
        &self.rest.input.data
    }

    /// Borrows matching rest matrices for selected-node or hierarchy poses.
    ///
    /// Apply current poses to the instance model's parts, including for bounds
    /// and material submissions, with the existing posed-part methods.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn pose(source: &GltfMorphModel) -> Result<(), String> {
    /// let hierarchy = source.pose().hierarchy()?;
    /// # Ok(()) }
    /// ```
    pub fn pose(&self) -> &GltfModelPose {
        &self.rest.pose
    }

    /// Creates independent authored weights, sharing immutable default buffers.
    ///
    /// No evaluation or upload happens here. Dropping this source afterwards is
    /// safe: the instance retains its shared rest data and texture owners.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn create(source: &GltfMorphModel) {
    /// let first = source.instantiate();
    /// let second = source.instantiate();
    /// assert_eq!(first.weights(0), second.weights(0));
    /// # }
    /// ```
    pub fn instantiate(&self) -> GltfMorphInstance {
        GltfMorphInstance {
            rest: self.rest.clone(),
            model: self.defaults.clone(),
            weights: self
                .data()
                .nodes
                .iter()
                .map(|node| node.weights.clone())
                .collect(),
        }
    }

    /// Creates a new source with the same axis conversion as `GltfModel`.
    ///
    /// This one-time operation copies/rotates CPU rest chunks and uploads new
    /// default buffers. Existing sources and instances remain independent.
    /// Invalid/zero axes and positive Y return a cheap shared clone.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn z_up(source: &GltfMorphModel) -> Result<GltfMorphModel, String> {
    /// source.with_up_axis(Vec3::Z)
    /// # }
    /// ```
    pub fn with_up_axis(&self, up: Vec3) -> Result<Self, String> {
        if crate::gltf::up_axis_rotation(up).is_none() {
            return Ok(self.clone());
        }
        Self::from_loaded(
            self.rest.model.clone().with_up_axis(up),
            self.rest.pose.with_up_axis(up),
            MorphLoadData {
                data: self.rest.input.data.clone(),
                indices: self.rest.input.indices.clone(),
                chunks: self
                    .rest
                    .input
                    .chunks
                    .iter()
                    .map(|chunk| MorphChunk {
                        node: chunk.node,
                        mesh: chunk.mesh,
                        primitive: chunk.primitive,
                        sources: chunk.sources.clone(),
                    })
                    .collect(),
            },
        )
    }
}

/// Independent morph weights and a current retained geometry snapshot.
///
/// Cloning shares immutable current GPU buffers and rest data, then edits are
/// independent. [`Self::set_weights`] evaluates only on a changed weight list.
/// Changed chunks replace immutable buffers rather than rewriting storage that
/// may already be queued. A queued draw or cloned [`RetainedGltfModel`] keeps
/// its old vertices, normals, textures and bounds alive through submission.
/// Dropping every instance after queuing is safe under retained-mesh retirement.
///
/// ```no_run
/// use macroquad::prelude::*;
/// # fn fork(instance: &GltfMorphInstance, node: usize) -> Result<(), String> {
/// let mut other = instance.clone();
/// let previous = other.model().clone();
/// other.set_weights(node, &[0.5])?;
/// previous.draw(Mat4::IDENTITY); // Captured old geometry remains valid.
/// other.model().draw(Mat4::from_translation(Vec3::X));
/// # Ok(()) }
/// ```
#[derive(Clone)]
pub struct GltfMorphInstance {
    rest: Arc<MorphRest>,
    model: RetainedGltfModel,
    weights: Vec<Vec<f32>>,
}

impl GltfMorphInstance {
    /// Reads the current retained model, with deformed normals and bounds.
    ///
    /// Its ordinary part, pose, culling and material-frame APIs are reusable.
    /// Immutable access prevents editing its geometry without matching bounds.
    /// Cloning it captures a stable snapshot, without CPU geometry retention.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn draw(instance: &GltfMorphInstance, camera: &Camera3D) {
    /// if let Some(frustum) = camera.frustum() {
    ///     instance.model().draw_culled(Mat4::IDENTITY, &frustum);
    /// }
    /// # }
    /// ```
    pub fn model(&self) -> &RetainedGltfModel {
        &self.model
    }

    /// Current scene-local node weights, including an empty list for unweighted
    /// nodes. A missing node returns `None`.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn inspect(instance: &GltfMorphInstance) {
    /// assert!(instance.weights(usize::MAX).is_none());
    /// # }
    /// ```
    pub fn weights(&self, node: usize) -> Option<&[f32]> {
        self.weights.get(node).map(Vec::as_slice)
    }

    /// Evaluates and uploads a changed node's POSITION/NORMAL morph weights.
    ///
    /// Returns `false` immediately for identical finite weights (including
    /// equivalent signed zeros), with no evaluation, allocation or GPU upload.
    /// Every weight must be finite and the count must match this node. Negative
    /// and extrapolated weights are allowed. Missing normals are regenerated
    /// from the deformed primitive's triangles before splitting into chunks.
    /// Authored normals follow the CPU evaluator and the node's inverse transpose.
    ///
    /// Errors leave weights, geometry and bounds unchanged. All affected CPU
    /// geometry and bounds are validated before any upload; successful updates
    /// create fresh immutable buffer pairs for affected chunks, even if a
    /// changed weight happens to produce the same positions. Existing static
    /// chunks and textures remain shared. No frame is flushed or presented.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn update(instance: &mut GltfMorphInstance, node: usize) -> Result<(), String> {
    /// instance.set_weights(node, &[0.75])?;
    /// assert!(!instance.set_weights(node, &[0.75])?);
    /// assert!(instance.set_weights(node, &[f32::NAN]).is_err());
    /// # Ok(()) }
    /// ```
    pub fn set_weights(&mut self, node: usize, weights: &[f32]) -> Result<bool, String> {
        let current = self
            .weights
            .get(node)
            .ok_or_else(|| format!("glTF morph node {node} does not exist"))?;
        if weights.len() != current.len() || weights.iter().any(|weight| !weight.is_finite()) {
            return Err(format!(
                "glTF morph node {node} requires {} finite weights",
                current.len()
            ));
        }
        if current == weights {
            return Ok(false);
        }
        let evaluated = evaluated_chunks(&self.rest, node, weights)?;
        // Validate every result before uploading or mutating instance state.
        let mut changed = Vec::with_capacity(evaluated.len());
        for (index, mesh) in evaluated {
            crate::models::validate_retained_mesh(&mesh).map_err(|e| e.to_string())?;
            let bounds = Aabb::from_points(mesh.vertices.iter().map(|v| v.position))
                .ok_or("glTF morph produced invalid bounds")?;
            changed.push((index, mesh, bounds));
        }
        let uploaded = changed
            .into_iter()
            .map(|(index, mesh, bounds)| {
                RetainedMesh::from_mesh(&mesh)
                    .map(|mesh| (index, mesh, bounds))
                    .map_err(|e| e.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        for (index, mesh, bounds) in uploaded {
            let part = &mut self.model.parts[index];
            part.mesh = mesh;
            part.bounds_min = bounds.min;
            part.bounds_max = bounds.max;
            part.center = bounds.min * 0.5 + bounds.max * 0.5;
        }
        self.weights[node] = weights.to_vec();
        let bounds = combined_bounds(&self.model);
        self.model.bounds_min = bounds.map_or(Vec3::ZERO, |bounds| bounds.min);
        self.model.bounds_max = bounds.map_or(Vec3::ZERO, |bounds| bounds.max);
        Ok(true)
    }

    /// Restores one node's authored default weights, with the same upload and
    /// error rules as [`Self::set_weights`].
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn reset(instance: &mut GltfMorphInstance, node: usize) -> Result<(), String> {
    /// instance.reset_weights(node)?;
    /// # Ok(()) }
    /// ```
    pub fn reset_weights(&mut self, node: usize) -> Result<bool, String> {
        let weights = self
            .rest
            .input
            .data
            .nodes
            .get(node)
            .ok_or_else(|| format!("glTF morph node {node} does not exist"))?
            .weights
            .clone();
        self.set_weights(node, &weights)
    }
}

fn combined_bounds(model: &RetainedGltfModel) -> Option<Aabb> {
    model
        .parts
        .iter()
        .map(|part| Aabb::from_min_max(part.bounds_min, part.bounds_max))
        .reduce(|a, b| Aabb::from_min_max(a.min.min(b.min), a.max.max(b.max)))
}

fn update_cpu_bounds(part: &mut super::GltfDrawMesh) -> Result<(), String> {
    let bounds = Aabb::from_points(part.mesh.vertices.iter().map(|v| v.position))
        .ok_or("glTF morph produced invalid bounds")?;
    part.bounds_min = bounds.min;
    part.bounds_max = bounds.max;
    part.center = bounds.min * 0.5 + bounds.max * 0.5;
    Ok(())
}

// Keep chunk correspondence private and captured at conversion, instead of
// guessing primitive identity from public names, counts or part ordering.
fn evaluated_chunks(
    rest: &MorphRest,
    node: usize,
    weights: &[f32],
) -> Result<Vec<(usize, crate::models::Mesh)>, String> {
    let Some(mesh) = rest.input.data.nodes[node].mesh else {
        return Ok(Vec::new());
    };
    let world = rest
        .pose
        .rest_world(node)
        .ok_or("missing glTF morph node world")?;
    let normal_matrix = world.inverse().transpose();
    let mut result = Vec::new();
    for (primitive, input) in rest.input.data.meshes[mesh].primitives.iter().enumerate() {
        // Padded empty targets are unaffected by any weights; they keep their
        // original immutable buffers, including UVs and generated rest normals.
        if input
            .targets
            .iter()
            .all(|target| target.position_deltas.is_empty() && target.normal_deltas.is_empty())
        {
            continue;
        }
        let evaluated = input.evaluate(weights).map_err(|e| e.to_string())?;
        let normals = if evaluated.normals.is_empty() {
            let mut normals = vec![Vec3::ZERO; evaluated.positions.len()];
            for triangle in rest.input.indices[mesh][primitive].chunks_exact(3) {
                let [a, b, c] = [
                    triangle[0] as usize,
                    triangle[1] as usize,
                    triangle[2] as usize,
                ];
                let normal = (evaluated.positions[b] - evaluated.positions[a])
                    .cross(evaluated.positions[c] - evaluated.positions[a]);
                normals[a] += normal;
                normals[b] += normal;
                normals[c] += normal;
            }
            if normals.iter().any(|normal| !normal.is_finite()) {
                return Err("glTF morph generated normals overflowed".into());
            }
            normals
        } else {
            evaluated.normals
        };
        for (part_index, chunk) in rest.input.chunks.iter().enumerate() {
            if chunk.node != node || chunk.mesh != mesh || chunk.primitive != primitive {
                continue;
            }
            let mut output = rest.model.parts[part_index].mesh.clone();
            for (vertex, &source) in output.vertices.iter_mut().zip(&chunk.sources) {
                vertex.position = world.transform_point3(evaluated.positions[source as usize]);
                vertex.normal = normal_matrix
                    .transform_vector3(normals[source as usize])
                    .normalize_or_zero()
                    .extend(0.);
                if !vertex.position.is_finite() || !vertex.normal.is_finite() {
                    return Err("glTF morph transformed geometry overflowed".into());
                }
            }
            result.push((part_index, output));
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gltf::{
        GltfMorphMesh, GltfMorphNode, GltfMorphPrimitive, GltfMorphTarget, GltfNode, GltfPrimitive,
        GltfScene,
    };
    use crate::gltf_draw::{
        GltfAlphaMode, GltfDrawMesh, GltfMeshLimits, GltfModelStats, GltfSurface,
    };
    use crate::models::Mesh;
    use glam::{vec2, vec3, Mat4, Quat};

    fn rest(world: Mat4, authored_normals: bool) -> MorphRest {
        let primitive = GltfPrimitive {
            positions: vec![Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::X + Vec3::Y],
            normals: if authored_normals {
                vec![Vec3::Z; 4]
            } else {
                Vec::new()
            },
            uvs: vec![vec2(0., 0.), vec2(1., 0.), vec2(0., 1.), vec2(1., 1.)],
            // A repeated/degenerate triangle and nonsequential order exercise
            // exact remapping and original-primitive smooth normal generation.
            indices: vec![2, 0, 1, 1, 3, 2, 0, 0, 0],
            base_color_factor: [0.2, 0.4, 0.8, 0.5],
            base_color_texture: None,
        };
        let input = GltfMorphPrimitive {
            positions: primitive.positions.clone(),
            normals: primitive.normals.clone(),
            targets: vec![GltfMorphTarget {
                position_deltas: vec![Vec3::ZERO, Vec3::ZERO, Vec3::Z, Vec3::Z],
                normal_deltas: if authored_normals {
                    vec![Vec3::Y; 4]
                } else {
                    Vec::new()
                },
            }],
        };
        let mut scene = GltfScene::from_bytes(br#"{"asset":{"version":"2.0"}}"#).unwrap();
        scene.nodes.push(GltfNode {
            name: Some("face".into()),
            parent: None,
            children: Vec::new(),
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
            world,
            mesh: Some(0),
        });
        let pose = GltfModelPose::from_scene(&scene);
        let vertices = super::super::world_vertices(&primitive, world);
        let indices = super::super::winding_indices(&primitive.indices, world);
        let mut model = GltfModel {
            parts: Vec::new(),
            stats: GltfModelStats::default(),
            bounds_min: Vec3::ZERO,
            bounds_max: Vec3::ONE,
            root_names: vec!["face".into()],
        };
        let mut chunks = Vec::new();
        for primitive_index in 0..2 {
            for (vertices, indices, sources) in super::super::split_triangles(
                &vertices,
                &indices,
                GltfMeshLimits {
                    vertices: 3,
                    indices: 3,
                },
            )
            .unwrap()
            {
                let bounds =
                    Aabb::from_points(vertices.iter().map(|vertex| vertex.position)).unwrap();
                model.parts.push(GltfDrawMesh {
                    mesh: Mesh {
                        vertices,
                        indices,
                        texture: None,
                    },
                    emissive_texture: None,
                    surface: GltfSurface {
                        name: None,
                        base_color: primitive.base_color_factor,
                        emissive: [0.; 3],
                        metallic: 0.,
                        roughness: 1.,
                        transmission: 0.,
                        alpha_mode: GltfAlphaMode::Blend,
                        alpha_cutoff: 0.5,
                        double_sided: false,
                        unlit: false,
                    },
                    node: 0,
                    name: "face".into(),
                    path: "face".into(),
                    center: bounds.center(),
                    bounds_min: bounds.min,
                    bounds_max: bounds.max,
                });
                chunks.push(MorphChunk {
                    node: 0,
                    mesh: 0,
                    primitive: primitive_index,
                    sources,
                });
            }
        }
        let mut static_primitive = input.clone();
        static_primitive.targets = vec![GltfMorphTarget::default()];
        MorphRest {
            model,
            pose,
            input: MorphLoadData {
                data: GltfMorphData {
                    nodes: vec![GltfMorphNode {
                        mesh: Some(0),
                        weights: vec![0.25],
                    }],
                    meshes: vec![GltfMorphMesh {
                        primitives: vec![input, static_primitive],
                        weights: vec![0.25],
                    }],
                },
                indices: vec![vec![primitive.indices.clone(), primitive.indices]],
                chunks,
            },
        }
    }

    #[test]
    fn deformed_chunks_keep_exact_sources_material_data_and_reflected_winding() {
        let mut world = Mat4::from_scale_rotation_translation(
            vec3(-2., 0.5, 3.),
            Quat::from_rotation_z(0.3),
            vec3(4., -2., 1.),
        );
        world.y_axis += world.x_axis * 0.25;
        let rest = rest(world, true);
        let original = rest.input.data.clone();
        let output = evaluated_chunks(&rest, 0, &[0.75]).unwrap();
        assert_eq!(output.len(), 3); // Static padded primitive never reuploads.
        let normal = world
            .inverse()
            .transpose()
            .transform_vector3(vec3(0., 0.75, 1.).normalize())
            .normalize();
        for (part, mesh) in output {
            let chunk = &rest.input.chunks[part];
            assert_eq!(mesh.indices, rest.model.parts[part].mesh.indices);
            for (index, vertex) in mesh.vertices.iter().enumerate() {
                let source = chunk.sources[index] as usize;
                let input = &rest.input.data.meshes[0].primitives[0];
                let local =
                    input.positions[source] + input.targets[0].position_deltas[source] * 0.75;
                assert!(vertex
                    .position
                    .abs_diff_eq(world.transform_point3(local), 1e-5));
                assert!(vertex.normal.truncate().abs_diff_eq(normal, 1e-5));
                assert_eq!(vertex.uv, rest.model.parts[part].mesh.vertices[index].uv);
                assert_eq!(
                    vertex.color,
                    rest.model.parts[part].mesh.vertices[index].color
                );
            }
        }
        assert_eq!(rest.input.data, original);
    }

    #[test]
    fn missing_authored_normals_follow_deformed_primitive_across_chunks() {
        let rest = rest(Mat4::IDENTITY, false);
        let output = evaluated_chunks(&rest, 0, &[1.]).unwrap();
        for (_, mesh) in output {
            for vertex in mesh.vertices {
                assert!(vertex
                    .normal
                    .truncate()
                    .abs_diff_eq(vec3(0., -1., 1.).normalize(), 1e-6));
            }
        }
        let singular = rest_with_singular_world();
        for (_, mesh) in evaluated_chunks(&singular, 0, &[1.]).unwrap() {
            for vertex in mesh.vertices {
                assert!(vertex.position.is_finite());
                assert_eq!(vertex.normal, glam::Vec4::ZERO);
            }
        }
    }

    fn rest_with_singular_world() -> MorphRest {
        rest(Mat4::from_scale(vec3(0., 1., 1.)), false)
    }

    #[test]
    fn morph_arithmetic_overflow_fails_before_producing_any_upload_input() {
        let rest = rest(Mat4::from_scale(Vec3::splat(16.)), true);
        let original = rest.input.data.clone();
        assert!(evaluated_chunks(&rest, 0, &[f32::MAX]).is_err());
        assert_eq!(rest.input.data, original);
        assert!(evaluated_chunks(&rest, 0, &[0.]).is_ok());
    }

    #[test]
    fn finite_huge_bounds_keep_a_finite_transparent_sort_center() {
        let mut rest = rest(Mat4::IDENTITY, true);
        rest.model.parts[0]
            .mesh
            .vertices
            .iter_mut()
            .for_each(|vertex| {
                vertex.position = Vec3::splat(2e38);
            });
        let part = &mut rest.model.parts[0];
        update_cpu_bounds(part).unwrap();
        assert!(part.center.is_finite());
        assert_eq!(part.center, Vec3::splat(2e38));
    }
}
