//! CPU-only mesh-local morph data, separate from the existing scene fields.

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

use glam::{Mat4, Vec3};

use super::{load_buffers, GltfError};
use crate::math::Aabb;

/// Mesh-local offsets for one morph target.
///
/// An empty array means zero deltas for that attribute. Tangent deltas are not
/// retained; a tangent-only target still occupies one weight slot.
///
/// ```
/// use macroquad::gltf::GltfMorphTarget;
/// use macroquad::math::Vec3;
/// let target = GltfMorphTarget {
///     position_deltas: vec![Vec3::Y],
///     normal_deltas: Vec::new(),
/// };
/// assert!(target.normal_deltas.is_empty());
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GltfMorphTarget {
    /// POSITION offsets, empty or parallel to the base positions.
    pub position_deltas: Vec<Vec3>,
    /// NORMAL offsets, empty or parallel to the base normals.
    pub normal_deltas: Vec<Vec3>,
}

/// Immutable input arrays for evaluating one primitive in mesh-local space.
///
/// Arrays remain public for inspecting or constructing CPU data. Evaluation
/// validates their lengths and finite values. Missing base normals stay empty;
/// normals are not generated. UVs, indices and materials remain on the matching
/// [`super::GltfPrimitive`]. Unaffected primitives have empty targets padded to
/// the mesh's weight count, so the same weights evaluate every primitive.
///
/// ```
/// use macroquad::gltf::{GltfMorphPrimitive, GltfMorphTarget};
/// use macroquad::math::Vec3;
/// let primitive = GltfMorphPrimitive {
///     positions: vec![Vec3::ZERO], normals: Vec::new(),
///     targets: vec![GltfMorphTarget {
///         position_deltas: vec![Vec3::Y], normal_deltas: Vec::new(),
///     }],
/// };
/// assert_eq!(primitive.evaluate(&[0.5]).unwrap().positions, vec![Vec3::Y * 0.5]);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct GltfMorphPrimitive {
    /// Base POSITION values.
    pub positions: Vec<Vec3>,
    /// Base NORMAL values, or empty when absent.
    pub normals: Vec<Vec3>,
    /// Targets in file order, including empty entries for unsupported tangents.
    pub targets: Vec<GltfMorphTarget>,
}

/// Morph primitives and default weights for one shared mesh.
///
/// `weights` has one value per target; absent file weights become zeros.
/// Node weights override these defaults without changing this shared mesh.
///
/// ```
/// use macroquad::gltf::GltfMorphMesh;
/// let mesh = GltfMorphMesh { primitives: Vec::new(), weights: vec![0.25] };
/// assert_eq!(mesh.weights, vec![0.25]);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct GltfMorphMesh {
    /// Parallel to the matching [`super::GltfMesh::primitives`].
    pub primitives: Vec<GltfMorphPrimitive>,
    /// Mesh weights, or zeros when omitted by the file.
    pub weights: Vec<f32>,
}

/// A scene node's shared mesh and resolved default weights.
///
/// Node-authored weights take precedence over mesh weights, then zeros. Nodes
/// without a mesh have an empty weight list. Indices match [`super::GltfScene`]
/// loaded from the same bytes and must stay with that source scene.
///
/// ```
/// use macroquad::gltf::GltfMorphNode;
/// let node = GltfMorphNode { mesh: Some(0), weights: vec![0.75] };
/// assert_eq!(node.mesh, Some(0));
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct GltfMorphNode {
    /// Index into [`GltfMorphData::meshes`], or no mesh.
    pub mesh: Option<usize>,
    /// Effective default weights for this node.
    pub weights: Vec<f32>,
}

/// CPU morph input for the same default scene selected by [`super::GltfScene`].
///
/// Meshes appear in first-use order and nodes in preorder. No images are decoded
/// and no GPU resources are created. This sidecar leaves existing scene struct
/// literals and draw behavior unchanged. It loads POSITION/NORMAL deltas only;
/// tangent morphs, skinning, animation clips and morph drawing are separate work.
///
/// ```
/// use macroquad::gltf::GltfMorphData;
/// let data = GltfMorphData::from_bytes(br#"{"asset":{"version":"2.0"}}"#).unwrap();
/// assert!(data.meshes.is_empty());
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct GltfMorphData {
    /// Parallel to [`super::GltfScene::nodes`] from the same document.
    pub nodes: Vec<GltfMorphNode>,
    /// Parallel to [`super::GltfScene::meshes`] from the same document.
    pub meshes: Vec<GltfMorphMesh>,
}

/// Evaluated vertex attributes and their mesh-local bounds.
///
/// Bounds enclose all evaluated vertices and therefore all triangles for these
/// weights; they are not a bound for arbitrary future weights. Output arrays
/// are owned, so another evaluation cannot change this result or its source.
///
/// ```
/// use macroquad::gltf::GltfMorphPrimitive;
/// use macroquad::math::Vec3;
/// let input = GltfMorphPrimitive {
///     positions: vec![Vec3::ZERO, Vec3::X], normals: Vec::new(), targets: Vec::new(),
/// };
/// let result = input.evaluate(&[]).unwrap();
/// assert_eq!(result.bounds.unwrap().max, Vec3::X);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct GltfMorphEvaluation {
    /// `base + sum(delta * weight)` in mesh-local space.
    pub positions: Vec<Vec3>,
    /// Evaluated normals, or empty when the base has none.
    pub normals: Vec<Vec3>,
    /// Bounds of [`Self::positions`], or no bounds for an empty primitive.
    pub bounds: Option<Aabb>,
}

impl GltfMorphPrimitive {
    /// Evaluates one finite weight per target, without clamping or renormalizing
    /// the weights. Negative weights and weights above one are supported.
    ///
    /// A missing normal delta preserves the base normal. When a normal target
    /// has nonzero weight, the resulting nonzero normal is normalized; a zero
    /// vector stays zero. Invalid array sizes, non-finite inputs and arithmetic
    /// overflow are errors, leaving the source unchanged.
    ///
    /// ```
    /// use macroquad::gltf::{GltfMorphPrimitive, GltfMorphTarget};
    /// use macroquad::math::Vec3;
    /// let input = GltfMorphPrimitive {
    ///     positions: vec![Vec3::X], normals: Vec::new(),
    ///     targets: vec![GltfMorphTarget { position_deltas: vec![Vec3::Y], ..Default::default() }],
    /// };
    /// assert_eq!(input.evaluate(&[2.0]).unwrap().positions[0], Vec3::X + Vec3::Y * 2.0);
    /// assert!(input.evaluate(&[f32::NAN]).is_err());
    /// ```
    pub fn evaluate(&self, weights: &[f32]) -> Result<GltfMorphEvaluation, GltfError> {
        check_weights(weights, self.targets.len(), "morph evaluation")?;
        check_vectors(&self.positions, self.positions.len(), "base positions")?;
        check_optional_vectors(&self.normals, self.positions.len(), "base normals")?;
        for target in &self.targets {
            check_optional_vectors(
                &target.position_deltas,
                self.positions.len(),
                "POSITION deltas",
            )?;
            check_optional_vectors(&target.normal_deltas, self.normals.len(), "NORMAL deltas")?;
        }
        let mut positions = self.positions.clone();
        let mut normals = self.normals.clone();
        let mut changed_normals = false;
        for (target, &weight) in self.targets.iter().zip(weights) {
            if weight == 0.0 {
                continue;
            }
            for (position, delta) in positions.iter_mut().zip(&target.position_deltas) {
                *position += *delta * weight;
            }
            for (normal, delta) in normals.iter_mut().zip(&target.normal_deltas) {
                *normal += *delta * weight;
            }
            changed_normals |= !target.normal_deltas.is_empty();
        }
        check_vectors(&positions, self.positions.len(), "evaluated positions")?;
        check_vectors(&normals, self.normals.len(), "evaluated normals")?;
        if changed_normals {
            for normal in &mut normals {
                // Scaling first avoids overflow/underflow in length_squared.
                let scale = normal.abs().max_element();
                if scale > 0.0 {
                    *normal = (*normal / scale).normalize();
                }
            }
        }
        let bounds = Aabb::from_points(positions.iter().copied());
        Ok(GltfMorphEvaluation {
            positions,
            normals,
            bounds,
        })
    }
}

impl GltfMorphEvaluation {
    /// Bounds the evaluated vertices after a finite affine transform.
    ///
    /// Use the matching scene node's world matrix (after its up-axis conversion
    /// or hierarchy edit). Morphs are evaluated before that transform. Mirrors,
    /// shear and singular affine transforms are supported. Projective matrices
    /// and overflow are errors. Normals remain mesh-local; drawing must apply
    /// the same inverse-transpose rule as the existing draw loader.
    ///
    /// ```
    /// use macroquad::gltf::GltfMorphPrimitive;
    /// use macroquad::math::{Mat4, Vec3};
    /// let result = GltfMorphPrimitive {
    ///     positions: vec![Vec3::X], normals: Vec::new(), targets: Vec::new(),
    /// }.evaluate(&[]).unwrap();
    /// let bounds = result.transformed_bounds(Mat4::from_translation(Vec3::Y)).unwrap().unwrap();
    /// assert_eq!(bounds.min, Vec3::X + Vec3::Y);
    /// ```
    pub fn transformed_bounds(&self, transform: Mat4) -> Result<Option<Aabb>, GltfError> {
        if !transform.is_finite()
            || transform.x_axis.w != 0.0
            || transform.y_axis.w != 0.0
            || transform.z_axis.w != 0.0
            || transform.w_axis.w != 1.0
        {
            return Err(GltfError::new(
                "morph bounds require a finite affine transform",
            ));
        }
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for &position in &self.positions {
            let world = transform.transform_point3(position);
            if !world.is_finite() {
                return Err(GltfError::new("morph transformed position is not finite"));
            }
            min = min.min(world);
            max = max.max(world);
        }
        Ok((!self.positions.is_empty()).then_some(Aabb::from_min_max(min, max)))
    }
}

impl GltfMorphData {
    /// Loads base positions/normals, POSITION/NORMAL targets and default weights.
    ///
    /// Accepts GLB or embedded-buffer glTF, using the scene loader's URI rules.
    /// Accessor types, counts, strides, byte ranges and sparse indices are checked
    /// before reading. An invalid selected graph or weight count is an error.
    /// Unreferenced meshes and images are ignored; base-color image files are
    /// not opened or decoded. Coordinates stay in mesh-local glTF axes.
    ///
    /// ```
    /// use macroquad::gltf::GltfMorphData;
    /// let data = GltfMorphData::from_bytes(include_bytes!("../../examples/assets/triangle.glb")).unwrap();
    /// assert_eq!(data.meshes[0].weights.len(), 0);
    /// let node = data.nodes.iter().position(|node| node.mesh.is_some()).unwrap();
    /// let result = data.evaluate(node, 0, None).unwrap();
    /// assert_eq!(result.positions.len(), 3);
    /// ```
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, GltfError> {
        if bytes.starts_with(b"glTF") {
            let glb = gltf::binary::Glb::from_slice(bytes).map_err(parse_error)?;
            let gltf = gltf::Gltf::from_slice(&glb.json).map_err(parse_error)?;
            Self::from_document(&gltf.document, glb.bin.as_deref())
        } else {
            let gltf = gltf::Gltf::from_slice(bytes).map_err(parse_error)?;
            Self::from_document(&gltf.document, None)
        }
    }

    /// Evaluates a node's primitive with caller weights, or its resolved defaults
    /// when `weights` is [`None`]. Node and mesh indices belong to this source.
    ///
    /// Results stay mesh-local and do not alter shared rest arrays or other
    /// nodes. An invalid node, missing mesh, primitive or weight count is an error.
    ///
    /// ```
    /// use macroquad::gltf::GltfMorphData;
    /// let data = GltfMorphData::from_bytes(include_bytes!("../../examples/assets/triangle.glb")).unwrap();
    /// let node = data.nodes.iter().position(|node| node.mesh.is_some()).unwrap();
    /// assert_eq!(data.evaluate(node, 0, None).unwrap(), data.evaluate(node, 0, Some(&[])).unwrap());
    /// assert!(data.evaluate(usize::MAX, 0, None).is_err());
    /// ```
    pub fn evaluate(
        &self,
        node: usize,
        primitive: usize,
        weights: Option<&[f32]>,
    ) -> Result<GltfMorphEvaluation, GltfError> {
        let node = self
            .nodes
            .get(node)
            .ok_or_else(|| GltfError::new("morph node index is out of range"))?;
        let mesh = node
            .mesh
            .and_then(|index| self.meshes.get(index))
            .ok_or_else(|| GltfError::new("morph node has no matching mesh"))?;
        let primitive = mesh
            .primitives
            .get(primitive)
            .ok_or_else(|| GltfError::new("morph primitive index is out of range"))?;
        primitive.evaluate(weights.unwrap_or(&node.weights))
    }

    pub(crate) fn from_document(
        document: &gltf::Document,
        binary: Option<&[u8]>,
    ) -> Result<Self, GltfError> {
        let buffers = load_buffers(document, binary)?;
        let mut data = Self {
            nodes: Vec::new(),
            meshes: Vec::new(),
        };
        let mut mesh_of = HashMap::new();
        let mut seen_nodes = HashSet::new();
        let mut pending = document
            .default_scene()
            .or_else(|| document.scenes().next())
            .map(|scene| scene.nodes().collect::<Vec<_>>())
            .unwrap_or_default();
        pending.reverse();
        while let Some(node) = pending.pop() {
            if !seen_nodes.insert(node.index()) {
                return Err(GltfError::new(format!(
                    "glTF node {} appears more than once in the scene graph",
                    node.index()
                )));
            }
            let mesh = if let Some(mesh) = node.mesh() {
                let index = if let Some(&index) = mesh_of.get(&mesh.index()) {
                    index
                } else {
                    let index = data.meshes.len();
                    data.meshes.push(load_mesh(&mesh, &buffers)?);
                    mesh_of.insert(mesh.index(), index);
                    index
                };
                Some(index)
            } else {
                None
            };
            let weights = match mesh {
                Some(index) => {
                    let defaults = &data.meshes[index].weights;
                    let weights = node.weights().unwrap_or(defaults).to_vec();
                    check_weights(
                        &weights,
                        defaults.len(),
                        &format!("glTF node {}", node.index()),
                    )?;
                    weights
                }
                None => {
                    if node.weights().is_some_and(|weights| !weights.is_empty()) {
                        return Err(GltfError::new(format!(
                            "glTF node {} has weights but no mesh",
                            node.index()
                        )));
                    }
                    Vec::new()
                }
            };
            data.nodes.push(GltfMorphNode { mesh, weights });
            pending.extend(node.children().collect::<Vec<_>>().into_iter().rev());
        }
        Ok(data)
    }
}

fn parse_error(error: impl std::fmt::Display) -> GltfError {
    GltfError::new(format!("glTF document could not be parsed: {error}"))
}

fn check_weights(weights: &[f32], count: usize, context: &str) -> Result<(), GltfError> {
    if weights.len() != count || weights.iter().any(|weight| !weight.is_finite()) {
        return Err(GltfError::new(format!(
            "{context} requires {count} finite morph weights, got {}",
            weights.len()
        )));
    }
    Ok(())
}

fn check_vectors(values: &[Vec3], count: usize, context: &str) -> Result<(), GltfError> {
    if values.len() != count || values.iter().any(|value| !value.is_finite()) {
        return Err(GltfError::new(format!(
            "{context} requires {count} finite VEC3 values, got {}",
            values.len()
        )));
    }
    Ok(())
}

fn check_optional_vectors(values: &[Vec3], count: usize, context: &str) -> Result<(), GltfError> {
    if values.is_empty() {
        Ok(())
    } else {
        check_vectors(values, count, context)
    }
}

fn load_mesh(mesh: &gltf::Mesh<'_>, buffers: &[Cow<'_, [u8]>]) -> Result<GltfMorphMesh, GltfError> {
    let mut target_count = None;
    for primitive in mesh.primitives() {
        let count = primitive.morph_targets().len();
        if count > 0 {
            if target_count.is_some_and(|previous| previous != count) {
                return Err(GltfError::new(format!(
                    "glTF mesh {} has inconsistent morph target counts",
                    mesh.index()
                )));
            }
            target_count = Some(count);
        }
    }
    let count = target_count.unwrap_or(0);
    let weights = mesh
        .weights()
        .map_or_else(|| vec![0.0; count], <[f32]>::to_vec);
    check_weights(&weights, count, &format!("glTF mesh {}", mesh.index()))?;
    let mut primitives = Vec::new();
    for primitive in mesh.primitives() {
        let context = format!("glTF mesh {} primitive {}", mesh.index(), primitive.index());
        if primitive.mode() != gltf::mesh::Mode::Triangles {
            return Err(GltfError::new(format!(
                "{context} uses {:?}; only triangles are loaded",
                primitive.mode()
            )));
        }
        let base = primitive
            .get(&gltf::Semantic::Positions)
            .ok_or_else(|| GltfError::new(format!("{context} has no positions")))?;
        let positions = read_vec3(&base, buffers, &format!("{context} base POSITION"))?;
        let normals = primitive
            .get(&gltf::Semantic::Normals)
            .map(|accessor| read_vec3(&accessor, buffers, &format!("{context} base NORMAL")))
            .transpose()?
            .unwrap_or_default();
        check_optional_vectors(&normals, positions.len(), &format!("{context} base NORMAL"))?;
        let mut targets = Vec::new();
        for (index, target) in primitive.morph_targets().enumerate() {
            let read = |accessor: Option<gltf::Accessor<'_>>,
                        semantic: &str,
                        size: usize|
             -> Result<Vec<Vec3>, GltfError> {
                let context = format!("{context} morph target {index} {semantic}");
                let Some(accessor) = accessor else {
                    return Ok(Vec::new());
                };
                if accessor.count() != size {
                    return Err(GltfError::new(format!(
                        "{context} has {} values for {size} base values",
                        accessor.count()
                    )));
                }
                read_vec3(&accessor, buffers, &context)
            };
            targets.push(GltfMorphTarget {
                position_deltas: read(target.positions(), "POSITION", positions.len())?,
                normal_deltas: read(target.normals(), "NORMAL", normals.len())?,
            });
        }
        if targets.is_empty() {
            targets.resize(count, GltfMorphTarget::default());
        }
        primitives.push(GltfMorphPrimitive {
            positions,
            normals,
            targets,
        });
    }
    Ok(GltfMorphMesh {
        primitives,
        weights,
    })
}

fn read_vec3(
    accessor: &gltf::Accessor<'_>,
    buffers: &[Cow<'_, [u8]>],
    context: &str,
) -> Result<Vec<Vec3>, GltfError> {
    let invalid = || {
        GltfError::new(format!(
            "{context} accessor {} must contain finite FLOAT VEC3 values",
            accessor.index()
        ))
    };
    if accessor.data_type() != gltf::accessor::DataType::F32
        || accessor.dimensions() != gltf::accessor::Dimensions::Vec3
        || accessor.normalized()
        || accessor.count() == 0
    {
        return Err(invalid());
    }
    let mut values = if let Some(view) = accessor.view() {
        let bytes = elements(
            &view,
            accessor.offset(),
            accessor.count(),
            12,
            4,
            buffers,
            context,
        )?;
        bytes
            .chunks(view.stride().unwrap_or(12))
            .map(vec3_bytes)
            .collect::<Vec<_>>()
    } else {
        vec![Vec3::ZERO; accessor.count()]
    };
    if let Some(sparse) = accessor.sparse() {
        if sparse.count() == 0 || sparse.count() > accessor.count() {
            return Err(GltfError::new(format!(
                "{context} has an invalid sparse count"
            )));
        }
        let indices = sparse.indices();
        let values_source = sparse.values();
        if indices.view().stride().is_some() || values_source.view().stride().is_some() {
            return Err(GltfError::new(format!(
                "{context} sparse views must not have byteStride"
            )));
        }
        let size = indices.index_type().size();
        let index_bytes = elements(
            &indices.view(),
            indices.offset(),
            sparse.count(),
            size,
            size,
            buffers,
            context,
        )?;
        let value_bytes = elements(
            &values_source.view(),
            values_source.offset(),
            sparse.count(),
            12,
            4,
            buffers,
            context,
        )?;
        let mut previous = None;
        for (index, value) in index_bytes
            .chunks_exact(size)
            .zip(value_bytes.chunks_exact(12))
        {
            let index = match size {
                1 => index[0] as usize,
                2 => u16::from_le_bytes(index.try_into().unwrap()) as usize,
                4 => u32::from_le_bytes(index.try_into().unwrap()) as usize,
                _ => unreachable!(),
            };
            if index >= values.len() || previous.is_some_and(|previous| previous >= index) {
                return Err(GltfError::new(format!(
                    "{context} sparse indices must increase strictly and stay in range"
                )));
            }
            values[index] = vec3_bytes(value);
            previous = Some(index);
        }
    }
    if values.iter().any(|value| !value.is_finite()) {
        return Err(invalid());
    }
    Ok(values)
}

fn vec3_bytes(bytes: &[u8]) -> Vec3 {
    Vec3::new(
        f32::from_le_bytes(bytes[0..4].try_into().unwrap()),
        f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        f32::from_le_bytes(bytes[8..12].try_into().unwrap()),
    )
}

pub(super) fn elements<'a>(
    view: &gltf::buffer::View<'_>,
    offset: usize,
    count: usize,
    size: usize,
    alignment: usize,
    buffers: &'a [Cow<'_, [u8]>],
    context: &str,
) -> Result<&'a [u8], GltfError> {
    let invalid = || {
        GltfError::new(format!(
            "{context} accessor byte range/stride/alignment is invalid"
        ))
    };
    let stride = view.stride().unwrap_or(size);
    if stride < size
        || !stride.is_multiple_of(alignment)
        || !offset.is_multiple_of(alignment)
        || !view.offset().is_multiple_of(alignment)
    {
        return Err(invalid());
    }
    let end = count
        .checked_sub(1)
        .and_then(|count| count.checked_mul(stride))
        .and_then(|bytes| offset.checked_add(bytes))
        .and_then(|bytes| bytes.checked_add(size))
        .ok_or_else(invalid)?;
    let view_end = view
        .offset()
        .checked_add(view.length())
        .ok_or_else(invalid)?;
    buffers
        .get(view.buffer().index())
        .and_then(|buffer| buffer.get(view.offset()..view_end))
        .and_then(|bytes| bytes.get(offset..end))
        .ok_or_else(invalid)
}
