//! glTF 2.0 scenes loaded from bytes.
//!
//! This module is behind the `gltf` feature. It parses `.glb` and `.gltf`
//! documents into plain data. It does not open a window or create GPU resources.
//! [`crate::gltf_draw::GltfModel`] turns a `.glb` into textured
//! [`crate::models::Mesh`] chunks, and that type needs a running macroquad window.
//!
//! ```
//! use macroquad::gltf::GltfScene;
//!
//! let scene = GltfScene::from_bytes(
//!     br#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[{"name":"player_spawn","translation":[1.0,2.0,3.0]}]}"#,
//! )
//! .unwrap();
//! assert_eq!(scene.spawns.len(), 1);
//! assert_eq!(
//!     scene.spawns[0].position,
//!     macroquad::math::vec3(1.0, 2.0, 3.0)
//! );
//! ```
//!
//! glTF is Y-up and right-handed, in meters. Blender is Z-up; its glTF exporter
//! converts to Y-up, and this loader stores those axes as written. It does not
//! apply a second conversion. Macroquad does not assume an up axis.
//!
//! Mesh vertices stay in mesh-local space. [`GltfNode::world`] is the node's
//! place in the scene. External buffer file URIs are rejected: the bytes passed
//! to [`GltfScene::from_bytes`] must be a `.glb` or a `.gltf` whose buffers are
//! `data:` URIs. That keeps parsing free of filesystem and window access.
//! A buffer longer than its declared `byteLength` is truncated. Only triangle
//! primitives are loaded, and a node index used more than once is an error.
//!
//! Colliders, triggers, and spawn points come from the nodes that were loaded.
//! Node `extras` are preferred: `mq_shape` is `box`, `sphere`, or `capsule`,
//! and `mq_role` is `collider`, `trigger`, or `spawn`. When `mq_role` is
//! absent, a name ending in `_collider`, `_trigger`, or `_spawn` supplies the
//! role. `capsule` is skipped until a capsule type exists. A `box` or `sphere`
//! is centered on the world-matrix translation. Its size is the length of each
//! column of that matrix's upper 3×3, in column order, so a parent's scale is
//! included and a negative scale stays positive. Those lengths are not turned
//! into a world-aligned bound of a rotated box: the box stays an [`Aabb`] of
//! that size. A collider or trigger with no `mq_shape` and a mesh becomes an
//! [`Aabb`] around the mesh positions after the node's world matrix. An empty
//! node with a role but no `mq_shape` and no mesh is not a collider. A spawn
//! stores the world translation and the node name, and ignores `mq_shape`.

use std::collections::{HashMap, HashSet};

use glam::{Mat4, Quat, Vec2, Vec3};

use crate::math::{Aabb, Sphere};

/// A parsed glTF scene: the default scene's nodes and the meshes they use.
///
/// Nodes are stored in preorder, parents before children. A document with no
/// scenes yields empty `nodes` and `meshes`. Meshes that no loaded node
/// references are left out. Images, skins, and animations are not loaded here.
/// [`crate::gltf_draw::GltfModel`] decodes embedded images when it draws a `.glb`.
/// [`Self::colliders`] and [`Self::spawns`] are the markers on those nodes,
/// in the same preorder. See the module docs for the markup.
///
/// # Example
///
/// ```
/// use macroquad::gltf::GltfScene;
///
/// let scene = GltfScene::from_bytes(
///     br#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[{"name":"room"}]}"#,
/// )
/// .unwrap();
/// assert_eq!(scene.nodes[0].name.as_deref(), Some("room"));
/// assert!(scene.meshes.is_empty());
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct GltfScene {
    /// Nodes of the default scene, in preorder.
    pub nodes: Vec<GltfNode>,
    /// Meshes referenced by [`Self::nodes`], in the order they are first used.
    pub meshes: Vec<GltfMesh>,
    /// Colliders and triggers, in node preorder.
    ///
    /// A trigger is a [`GltfCollider`] with [`GltfCollider::trigger`] set.
    pub colliders: Vec<GltfCollider>,
    /// Spawn points, in node preorder.
    pub spawns: Vec<GltfSpawn>,
}

/// One node in a [`GltfScene`].
///
/// `translation`, `rotation`, and `scale` are the node's local transform.
/// `world` is that transform composed with every ancestor. A node whose glTF
/// source is a matrix is decomposed into those three fields; `world` is built
/// from the original matrix, so it still matches the file when the matrix
/// does not decompose cleanly.
///
/// # Example
///
/// ```
/// use macroquad::gltf::GltfNode;
/// use macroquad::math::{Mat4, Quat, Vec3};
///
/// let node = GltfNode {
///     name: Some("player_spawn".into()),
///     parent: None,
///     children: Vec::new(),
///     translation: Vec3::new(4.0, 0.0, 1.0),
///     rotation: Quat::IDENTITY,
///     scale: Vec3::ONE,
///     world: Mat4::from_translation(Vec3::new(4.0, 0.0, 1.0)),
///     mesh: None,
/// };
/// let (_scale, _rotation, translation) = node.world.to_scale_rotation_translation();
/// assert_eq!(translation, node.translation);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct GltfNode {
    /// glTF name, or [`None`] when the node has none.
    pub name: Option<String>,
    /// Index into [`GltfScene::nodes`] of the parent. [`None`] for a root.
    pub parent: Option<usize>,
    /// Indices into [`GltfScene::nodes`] of the children, in glTF order.
    pub children: Vec<usize>,
    /// Local translation.
    pub translation: Vec3,
    /// Local rotation.
    pub rotation: Quat,
    /// Local scale.
    pub scale: Vec3,
    /// Local transform composed with the parent [`Self::world`].
    pub world: Mat4,
    /// Index into [`GltfScene::meshes`], or [`None`] when the node has no mesh.
    pub mesh: Option<usize>,
}

/// A mesh referenced by one or more [`GltfNode`]s.
///
/// # Example
///
/// ```
/// use macroquad::gltf::GltfMesh;
///
/// let mesh = GltfMesh {
///     name: Some("tri".into()),
///     primitives: Vec::new(),
/// };
/// assert_eq!(mesh.name.as_deref(), Some("tri"));
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct GltfMesh {
    /// glTF mesh name, or [`None`] when the mesh has none.
    pub name: Option<String>,
    /// Primitives in file order.
    pub primitives: Vec<GltfPrimitive>,
}

/// One glTF mesh primitive, in mesh-local space.
///
/// `positions`, `normals`, and `uvs` are parallel when the accessor is
/// present. Missing normals or the first texture-coordinate set produce an
/// empty list, not generated data. A primitive with no index accessor gets
/// `0..positions.len()` so every primitive is indexed. The index count is a
/// multiple of 3, and every index is in range. Other primitive modes are an
/// error. `base_color_factor` is the metallic-roughness factor, RGBA,
/// defaulting to white when the material omits it. Normals and uvs, when
/// present, have one entry per position.
///
/// # Example
///
/// ```
/// use macroquad::gltf::GltfPrimitive;
/// use macroquad::math::{vec2, vec3};
///
/// let primitive = GltfPrimitive {
///     positions: vec![vec3(0.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0)],
///     normals: vec![vec3(0.0, 0.0, 1.0); 3],
///     uvs: vec![vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0)],
///     indices: vec![0, 1, 2],
///     base_color_factor: [1.0, 1.0, 1.0, 1.0],
/// };
/// assert_eq!(primitive.indices.len(), 3);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct GltfPrimitive {
    /// `POSITION` values.
    pub positions: Vec<Vec3>,
    /// `NORMAL` values, or empty when the primitive has none.
    pub normals: Vec<Vec3>,
    /// `TEXCOORD_0` values, or empty when the primitive has none.
    pub uvs: Vec<Vec2>,
    /// Triangle indices. See the struct docs for non-indexed primitives.
    pub indices: Vec<u32>,
    /// Metallic-roughness base color factor, linear RGBA.
    pub base_color_factor: [f32; 4],
}

/// Box or sphere produced from a glTF marker.
///
/// A rotated `box` stays an [`Aabb`] sized by the world-matrix column lengths.
/// It is not an oriented box. See the module docs.
///
/// # Example
///
/// ```
/// use macroquad::gltf::GltfColliderShape;
/// use macroquad::math::{Aabb, vec3};
///
/// let shape = GltfColliderShape::Aabb(Aabb::from_center_size(
///     vec3(0.0, 1.0, 0.0),
///     vec3(2.0, 1.0, 0.5),
/// ));
/// match shape {
///     GltfColliderShape::Aabb(aabb) => assert_eq!(aabb.size(), vec3(2.0, 1.0, 0.5)),
///     GltfColliderShape::Sphere(_) => panic!("expected a box"),
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GltfColliderShape {
    /// Axis-aligned box. The size matches [`Aabb::from_center_size`].
    Aabb(Aabb),
    /// Sphere. The radius is the largest column length, not half of it.
    Sphere(Sphere),
}

/// A collider or trigger taken from one glTF node.
///
/// # Example
///
/// ```
/// use macroquad::gltf::{GltfCollider, GltfColliderShape};
/// use macroquad::math::{Sphere, vec3};
///
/// let trigger = GltfCollider {
///     node: 0,
///     name: Some("goal_trigger".into()),
///     trigger: true,
///     shape: GltfColliderShape::Sphere(Sphere::new(vec3(0.0, 1.0, 0.0), 0.5)),
/// };
/// assert!(trigger.trigger);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct GltfCollider {
    /// Index into [`GltfScene::nodes`], which is not the index in the file
    /// when some nodes were left out of the scene.
    pub node: usize,
    /// Node name, or [`None`] when the node has none.
    pub name: Option<String>,
    /// True when the role is `trigger`. The shape is the same as a collider.
    pub trigger: bool,
    /// World-space shape.
    pub shape: GltfColliderShape,
}

/// A spawn point taken from one glTF node.
///
/// # Example
///
/// ```
/// use macroquad::gltf::GltfSpawn;
/// use macroquad::math::vec3;
///
/// let spawn = GltfSpawn {
///     node: 1,
///     name: Some("player_spawn".into()),
///     position: vec3(4.0, 0.0, 1.0),
/// };
/// assert_eq!(spawn.position, vec3(4.0, 0.0, 1.0));
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct GltfSpawn {
    /// Index into [`GltfScene::nodes`]. See [`GltfCollider::node`].
    pub node: usize,
    /// Node name, or [`None`] when the node has none.
    pub name: Option<String>,
    /// Translation column of the node's world matrix.
    pub position: Vec3,
}

/// Failure while reading a glTF document from bytes.
///
/// # Example
///
/// ```
/// use macroquad::gltf::GltfScene;
///
/// let error = GltfScene::from_bytes(b"this is not gltf").unwrap_err();
/// assert!(error.to_string().contains("could not be parsed"));
/// ```
#[derive(Debug)]
pub struct GltfError {
    message: String,
}

impl GltfError {
    fn new(message: impl Into<String>) -> GltfError {
        GltfError {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for GltfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GltfError {}

impl GltfScene {
    /// Parses a `.glb` or `.gltf` document from memory.
    ///
    /// The default scene is loaded. When `scene` is omitted, the first scene
    /// is used. No scene means an empty [`GltfScene`]. External buffer files
    /// are an error; see the module docs. A node that appears twice in that
    /// scene, including through a cycle, is an error. Markers are collected
    /// from the nodes that load; see the module docs.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::gltf::{GltfColliderShape, GltfScene};
    /// use macroquad::math::vec3;
    ///
    /// let scene = GltfScene::from_bytes(
    ///     br#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0,1]}],"nodes":[
    ///         {"name":"crate_collider","translation":[0.0,1.0,0.0],"scale":[2.0,1.0,0.5],
    ///          "extras":{"mq_shape":"box","mq_role":"collider"}},
    ///         {"name":"player_spawn","translation":[4.0,0.0,1.0]}
    ///     ]}"#,
    /// )
    /// .unwrap();
    /// assert_eq!(scene.spawns[0].position, vec3(4.0, 0.0, 1.0));
    /// match scene.colliders[0].shape {
    ///     GltfColliderShape::Aabb(aabb) => assert_eq!(aabb.size(), vec3(2.0, 1.0, 0.5)),
    ///     GltfColliderShape::Sphere(_) => panic!("expected a box"),
    /// }
    /// ```
    pub fn from_bytes(bytes: &[u8]) -> Result<GltfScene, GltfError> {
        let gltf = gltf::Gltf::from_slice(bytes).map_err(|error| {
            GltfError::new(format!("glTF document could not be parsed: {error}"))
        })?;
        let buffers = load_buffers(&gltf)?;
        let Some(scene) = gltf.default_scene().or_else(|| gltf.scenes().next()) else {
            return Ok(GltfScene {
                nodes: Vec::new(),
                meshes: Vec::new(),
                colliders: Vec::new(),
                spawns: Vec::new(),
            });
        };

        let mut loaded = GltfScene {
            nodes: Vec::new(),
            meshes: Vec::new(),
            colliders: Vec::new(),
            spawns: Vec::new(),
        };
        let mut mesh_of = HashMap::new();
        let mut seen_nodes = HashSet::new();
        for root in scene.nodes() {
            append_node(
                &mut loaded,
                &root,
                None,
                Mat4::IDENTITY,
                &buffers,
                &mut mesh_of,
                &mut seen_nodes,
            )?;
        }
        Ok(loaded)
    }
}

fn load_buffers(gltf: &gltf::Gltf) -> Result<Vec<Vec<u8>>, GltfError> {
    let mut blob = gltf.blob.clone();
    let mut buffers = Vec::new();
    for buffer in gltf.buffers() {
        let mut data = match buffer.source() {
            gltf::buffer::Source::Bin => blob.take().ok_or_else(|| {
                GltfError::new(format!(
                    "glTF buffer {} needs the binary chunk, which is missing",
                    buffer.index()
                ))
            })?,
            gltf::buffer::Source::Uri(uri) => decode_data_uri(uri)?,
        };
        if data.len() < buffer.length() {
            return Err(GltfError::new(format!(
                "glTF buffer {} is {} bytes, but the document says {}",
                buffer.index(),
                data.len(),
                buffer.length()
            )));
        }
        // GLB pads the binary chunk to 4 bytes. Accessors must not read that
        // padding, so the slice stops at the declared length.
        data.truncate(buffer.length());
        buffers.push(data);
    }
    Ok(buffers)
}

fn decode_data_uri(uri: &str) -> Result<Vec<u8>, GltfError> {
    let Some(rest) = uri.strip_prefix("data:") else {
        return Err(GltfError::new(
            "external glTF buffer files are not loaded from a byte slice; embed the buffer as a data URI or use a .glb",
        ));
    };
    let Some((header, payload)) = rest.split_once(',') else {
        return Err(GltfError::new("glTF buffer data URI has no payload"));
    };
    if !header.contains("base64") {
        return Err(GltfError::new("glTF buffer data URI is not base64"));
    }
    decode_base64(payload).ok_or_else(|| GltfError::new("glTF buffer data URI is not valid base64"))
}

fn decode_base64(input: &str) -> Option<Vec<u8>> {
    fn value(byte: u8) -> Option<u8> {
        match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let bytes: Vec<u8> = input
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if bytes.is_empty() || !bytes.len().is_multiple_of(4) {
        return None;
    }

    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.as_chunks::<4>().0 {
        let (a, b, c, d) = (chunk[0], chunk[1], chunk[2], chunk[3]);
        let a = value(a)?;
        let b = value(b)?;
        let c_pad = c == b'=';
        let d_pad = d == b'=';
        if c_pad && !d_pad {
            return None;
        }
        let c = if c_pad { 0 } else { value(c)? };
        let d = if d_pad { 0 } else { value(d)? };
        let triple = ((a as u32) << 18) | ((b as u32) << 12) | ((c as u32) << 6) | (d as u32);
        out.push((triple >> 16) as u8);
        if !c_pad {
            out.push((triple >> 8) as u8);
        }
        if !d_pad {
            out.push(triple as u8);
        }
    }
    Some(out)
}

fn append_node(
    loaded: &mut GltfScene,
    node: &gltf::Node<'_>,
    parent: Option<usize>,
    parent_world: Mat4,
    buffers: &[Vec<u8>],
    mesh_of: &mut HashMap<usize, usize>,
    seen_nodes: &mut HashSet<usize>,
) -> Result<usize, GltfError> {
    if !seen_nodes.insert(node.index()) {
        return Err(GltfError::new(format!(
            "glTF node {} appears more than once in the scene graph",
            node.index()
        )));
    }
    let (translation, rotation, scale, local) = local_transform(node);
    let world = parent_world * local;
    let mesh = match node.mesh() {
        Some(mesh) => Some(mesh_index(loaded, &mesh, buffers, mesh_of)?),
        None => None,
    };
    let index = loaded.nodes.len();
    loaded.nodes.push(GltfNode {
        name: node.name().map(str::to_owned),
        parent,
        children: Vec::new(),
        translation,
        rotation,
        scale,
        world,
        mesh,
    });
    record_marker(loaded, node, index);

    let children: Vec<gltf::Node<'_>> = node.children().collect();
    let mut child_indices = Vec::with_capacity(children.len());
    for child in &children {
        child_indices.push(append_node(
            loaded,
            child,
            Some(index),
            world,
            buffers,
            mesh_of,
            seen_nodes,
        )?);
    }
    loaded.nodes[index].children = child_indices;
    Ok(index)
}

fn local_transform(node: &gltf::Node<'_>) -> (Vec3, Quat, Vec3, Mat4) {
    match node.transform() {
        gltf::scene::Transform::Decomposed {
            translation,
            rotation,
            scale,
        } => {
            let translation = Vec3::from(translation);
            let rotation = Quat::from_xyzw(rotation[0], rotation[1], rotation[2], rotation[3]);
            let scale = Vec3::from(scale);
            let local = Mat4::from_scale_rotation_translation(scale, rotation, translation);
            (translation, rotation, scale, local)
        }
        gltf::scene::Transform::Matrix { matrix } => {
            let local = Mat4::from_cols_array_2d(&matrix);
            let (scale, rotation, translation) = local.to_scale_rotation_translation();
            (translation, rotation, scale, local)
        }
    }
}

fn mesh_index(
    loaded: &mut GltfScene,
    mesh: &gltf::Mesh<'_>,
    buffers: &[Vec<u8>],
    mesh_of: &mut HashMap<usize, usize>,
) -> Result<usize, GltfError> {
    if let Some(index) = mesh_of.get(&mesh.index()) {
        return Ok(*index);
    }
    let mut primitives = Vec::new();
    for (primitive_index, primitive) in mesh.primitives().enumerate() {
        primitives.push(load_primitive(
            mesh.index(),
            primitive_index,
            &primitive,
            buffers,
        )?);
    }
    let index = loaded.meshes.len();
    loaded.meshes.push(GltfMesh {
        name: mesh.name().map(str::to_owned),
        primitives,
    });
    mesh_of.insert(mesh.index(), index);
    Ok(index)
}

fn load_primitive(
    mesh_index: usize,
    primitive_index: usize,
    primitive: &gltf::Primitive<'_>,
    buffers: &[Vec<u8>],
) -> Result<GltfPrimitive, GltfError> {
    if primitive.mode() != gltf::mesh::Mode::Triangles {
        return Err(GltfError::new(format!(
            "glTF mesh {mesh_index} primitive {primitive_index} uses {:?}; only triangles are loaded",
            primitive.mode()
        )));
    }

    let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(Vec::as_slice));
    let positions: Vec<Vec3> = reader
        .read_positions()
        .ok_or_else(|| {
            GltfError::new(format!(
                "glTF mesh {mesh_index} primitive {primitive_index} has no positions"
            ))
        })?
        .map(Vec3::from)
        .collect();
    let normals: Vec<Vec3> = reader
        .read_normals()
        .map(|normals| normals.map(Vec3::from).collect())
        .unwrap_or_default();
    let uvs: Vec<Vec2> = reader
        .read_tex_coords(0)
        .map(|coords| coords.into_f32().map(Vec2::from).collect())
        .unwrap_or_default();
    let indices: Vec<u32> = match reader.read_indices() {
        Some(indices) => indices.into_u32().collect(),
        None => (0..positions.len() as u32).collect(),
    };
    if !indices.len().is_multiple_of(3) {
        return Err(GltfError::new(format!(
            "glTF mesh {mesh_index} primitive {primitive_index} has {} indices, which is not a multiple of 3",
            indices.len()
        )));
    }
    if indices
        .iter()
        .any(|index| *index as usize >= positions.len())
    {
        return Err(GltfError::new(format!(
            "glTF mesh {mesh_index} primitive {primitive_index} has an index outside its {} positions",
            positions.len()
        )));
    }
    if !(normals.is_empty() || normals.len() == positions.len()) {
        return Err(GltfError::new(format!(
            "glTF mesh {mesh_index} primitive {primitive_index} has {} normals for {} positions",
            normals.len(),
            positions.len()
        )));
    }
    if !(uvs.is_empty() || uvs.len() == positions.len()) {
        return Err(GltfError::new(format!(
            "glTF mesh {mesh_index} primitive {primitive_index} has {} uvs for {} positions",
            uvs.len(),
            positions.len()
        )));
    }
    Ok(GltfPrimitive {
        positions,
        normals,
        uvs,
        indices,
        base_color_factor: primitive
            .material()
            .pbr_metallic_roughness()
            .base_color_factor(),
    })
}

#[derive(Clone, Copy)]
enum MarkerRole {
    Collider,
    Trigger,
    Spawn,
}

#[derive(Clone, Copy)]
enum MarkerShape {
    Box,
    Sphere,
    Capsule,
}

fn record_marker(loaded: &mut GltfScene, node: &gltf::Node<'_>, index: usize) {
    let Some(role) = marker_role(node) else {
        return;
    };
    let name = loaded.nodes[index].name.clone();
    let world = loaded.nodes[index].world;
    match role {
        MarkerRole::Spawn => {
            let position = world.transform_point3(Vec3::ZERO);
            if position.is_finite() {
                loaded.spawns.push(GltfSpawn {
                    node: index,
                    name,
                    position,
                });
            }
        }
        MarkerRole::Collider | MarkerRole::Trigger => {
            let Some(shape) = marker_shape(loaded, node, index, world) else {
                return;
            };
            loaded.colliders.push(GltfCollider {
                node: index,
                name,
                trigger: matches!(role, MarkerRole::Trigger),
                shape,
            });
        }
    }
}

fn marker_role(node: &gltf::Node<'_>) -> Option<MarkerRole> {
    if let Some(role) = role_word(extras_field(node, "mq_role").as_deref()) {
        return Some(role);
    }
    node.name().and_then(suffix_role)
}

fn role_word(value: Option<&str>) -> Option<MarkerRole> {
    match value? {
        "collider" => Some(MarkerRole::Collider),
        "trigger" => Some(MarkerRole::Trigger),
        "spawn" => Some(MarkerRole::Spawn),
        _ => None,
    }
}

fn suffix_role(name: &str) -> Option<MarkerRole> {
    if name.ends_with("_collider") {
        Some(MarkerRole::Collider)
    } else if name.ends_with("_trigger") {
        Some(MarkerRole::Trigger)
    } else if name.ends_with("_spawn") {
        Some(MarkerRole::Spawn)
    } else {
        None
    }
}

fn explicit_shape(node: &gltf::Node<'_>) -> Option<MarkerShape> {
    match extras_field(node, "mq_shape").as_deref() {
        Some("box") => Some(MarkerShape::Box),
        Some("sphere") => Some(MarkerShape::Sphere),
        Some("capsule") => Some(MarkerShape::Capsule),
        _ => None,
    }
}

fn marker_shape(
    loaded: &GltfScene,
    node: &gltf::Node<'_>,
    index: usize,
    world: Mat4,
) -> Option<GltfColliderShape> {
    match explicit_shape(node) {
        Some(MarkerShape::Capsule) => None,
        Some(MarkerShape::Box) => world_extent(world)
            .map(|(center, size)| GltfColliderShape::Aabb(Aabb::from_center_size(center, size))),
        Some(MarkerShape::Sphere) => world_extent(world).map(|(center, size)| {
            GltfColliderShape::Sphere(Sphere::new(center, size.max_element()))
        }),
        None => loaded.nodes[index]
            .mesh
            .and_then(|mesh| mesh_aabb(loaded, mesh, world))
            .map(GltfColliderShape::Aabb),
    }
}

/// Center and full size from the world matrix.
///
/// `size` is the length of each upper 3×3 column, in column order. A non-finite
/// center or column length yields [`None`].
fn world_extent(world: Mat4) -> Option<(Vec3, Vec3)> {
    let center = world.transform_point3(Vec3::ZERO);
    let size = Vec3::new(
        world.x_axis.truncate().length(),
        world.y_axis.truncate().length(),
        world.z_axis.truncate().length(),
    );
    if center.is_finite() && size.is_finite() {
        Some((center, size))
    } else {
        None
    }
}

fn mesh_aabb(loaded: &GltfScene, mesh: usize, world: Mat4) -> Option<Aabb> {
    let mut points = Vec::new();
    for primitive in &loaded.meshes[mesh].primitives {
        for position in &primitive.positions {
            let point = world.transform_point3(*position);
            if !point.is_finite() {
                return None;
            }
            points.push(point);
        }
    }
    Aabb::from_points(points)
}

fn extras_field(node: &gltf::Node<'_>, key: &str) -> Option<String> {
    let raw = node.extras().as_ref()?;
    json_object_string(raw.get(), key)
}

/// Top-level string field of a JSON object.
///
/// A non-object, a missing key, or a value that is not a string yields
/// [`None`]. Nested objects and arrays are skipped, so a key inside them is
/// not a match.
fn json_object_string(json: &str, key: &str) -> Option<String> {
    let mut cursor = JsonCursor { rest: json };
    cursor.skip_ws();
    if cursor.bump() != Some('{') {
        return None;
    }
    loop {
        cursor.skip_ws();
        if cursor.peek() == Some('}') {
            return None;
        }
        let found = cursor.parse_string()?;
        cursor.skip_ws();
        if cursor.bump() != Some(':') {
            return None;
        }
        cursor.skip_ws();
        if found == key {
            return cursor.parse_string();
        }
        if !cursor.skip_value() {
            return None;
        }
        cursor.skip_ws();
        match cursor.bump() {
            Some(',') => continue,
            _ => return None,
        }
    }
}

struct JsonCursor<'a> {
    rest: &'a str,
}

impl<'a> JsonCursor<'a> {
    fn peek(&self) -> Option<char> {
        self.rest.chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let mut chars = self.rest.chars();
        let next = chars.next()?;
        self.rest = chars.as_str();
        Some(next)
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    fn parse_string(&mut self) -> Option<String> {
        if self.bump() != Some('"') {
            return None;
        }
        let mut out = String::new();
        loop {
            match self.bump()? {
                '"' => return Some(out),
                '\\' => out.push(self.unescape()?),
                ch => out.push(ch),
            }
        }
    }

    fn unescape(&mut self) -> Option<char> {
        match self.bump()? {
            '"' => Some('"'),
            '\\' => Some('\\'),
            '/' => Some('/'),
            'b' => Some('\u{0008}'),
            'f' => Some('\u{000c}'),
            'n' => Some('\n'),
            'r' => Some('\r'),
            't' => Some('\t'),
            'u' => {
                let mut hex = String::new();
                for _ in 0..4 {
                    let ch = self.bump()?;
                    if !ch.is_ascii_hexdigit() {
                        return None;
                    }
                    hex.push(ch);
                }
                char::from_u32(u32::from_str_radix(&hex, 16).ok()?)
            }
            _ => None,
        }
    }

    fn skip_value(&mut self) -> bool {
        self.skip_ws();
        match self.peek() {
            Some('"') => self.parse_string().is_some(),
            Some('{') => self.skip_container('{', '}'),
            Some('[') => self.skip_container('[', ']'),
            Some('-' | '0'..='9') => self.skip_number(),
            Some('t') => self.consume("true"),
            Some('f') => self.consume("false"),
            Some('n') => self.consume("null"),
            _ => false,
        }
    }

    fn consume(&mut self, literal: &str) -> bool {
        match self.rest.strip_prefix(literal) {
            Some(rest) => {
                self.rest = rest;
                true
            }
            None => false,
        }
    }

    fn skip_container(&mut self, open: char, close: char) -> bool {
        if self.bump() != Some(open) {
            return false;
        }
        let mut depth = 1;
        while depth > 0 {
            match self.bump() {
                Some(ch) if ch == open => depth += 1,
                Some(ch) if ch == close => depth -= 1,
                Some('"') => {
                    if self.skip_string_body().is_none() {
                        return false;
                    }
                }
                Some(_) => {}
                None => return false,
            }
        }
        true
    }

    fn skip_string_body(&mut self) -> Option<()> {
        loop {
            match self.bump()? {
                '"' => return Some(()),
                '\\' => {
                    self.bump()?;
                }
                _ => {}
            }
        }
    }

    fn skip_number(&mut self) -> bool {
        if self.peek() == Some('-') {
            self.bump();
        }
        match self.peek() {
            Some('0') => {
                self.bump();
            }
            Some('1'..='9') => {
                while matches!(self.peek(), Some('0'..='9')) {
                    self.bump();
                }
            }
            _ => return false,
        }
        if self.peek() == Some('.') {
            self.bump();
            if !matches!(self.peek(), Some('0'..='9')) {
                return false;
            }
            while matches!(self.peek(), Some('0'..='9')) {
                self.bump();
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            self.bump();
            if matches!(self.peek(), Some('+' | '-')) {
                self.bump();
            }
            if !matches!(self.peek(), Some('0'..='9')) {
                return false;
            }
            while matches!(self.peek(), Some('0'..='9')) {
                self.bump();
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::vec3;

    fn assert_vec3_close(left: Vec3, right: Vec3) {
        let delta = left - right;
        assert!(
            delta.x.abs() < 1e-5 && delta.y.abs() < 1e-5 && delta.z.abs() < 1e-5,
            "left {left:?} right {right:?}"
        );
    }

    fn assert_mat4_close(left: Mat4, right: Mat4) {
        let left_cols = left.to_cols_array();
        let right_cols = right.to_cols_array();
        for (left, right) in left_cols.iter().zip(right_cols.iter()) {
            assert!(
                (left - right).abs() < 1e-5,
                "left {left_cols:?} right {right_cols:?}"
            );
        }
    }

    fn node<'a>(scene: &'a GltfScene, name: &str) -> &'a GltfNode {
        scene
            .nodes
            .iter()
            .find(|node| node.name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("missing node {name}"))
    }

    #[test]
    fn sample_gltf_and_glb_have_the_same_hierarchy() {
        let gltf = GltfScene::from_bytes(include_bytes!("../examples/assets/triangle.gltf"))
            .expect("triangle.gltf");
        let glb = GltfScene::from_bytes(include_bytes!("../examples/assets/triangle.glb"))
            .expect("triangle.glb");
        assert_eq!(gltf, glb);

        let names: Vec<_> = gltf.nodes.iter().map(|node| node.name.as_deref()).collect();
        assert_eq!(
            names,
            vec![
                Some("root"),
                Some("box"),
                Some("spin"),
                Some("box_copy"),
                Some("crate_collider"),
                Some("marker"),
                Some("player_spawn"),
            ]
        );
        assert!(gltf
            .nodes
            .iter()
            .all(|node| node.name.as_deref() != Some("hidden")));

        let root = node(&gltf, "root");
        let box_node = node(&gltf, "box");
        let spin = node(&gltf, "spin");
        let copy = node(&gltf, "box_copy");
        let marker = node(&gltf, "marker");
        assert_eq!(root.parent, None);
        assert_eq!(marker.parent, None);
        assert_eq!(box_node.parent, Some(0));
        assert_eq!(root.children, vec![1, 2, 3, 4]);
        assert_eq!(box_node.translation, vec3(0.0, 1.0, 0.0));
        assert_eq!(box_node.scale, vec3(2.0, 2.0, 2.0));
        assert_eq!(box_node.rotation, Quat::IDENTITY);
        assert_eq!(root.mesh, None);
        assert_eq!(box_node.mesh, Some(0));
        assert_eq!(copy.mesh, Some(0));
        assert_eq!(gltf.meshes.len(), 1);
        assert_eq!(gltf.meshes[0].name.as_deref(), Some("tri"));

        let root_world = Mat4::from_translation(vec3(1.0, 2.0, 3.0));
        assert_eq!(root.world, root_world);
        let box_local = Mat4::from_scale_rotation_translation(
            vec3(2.0, 2.0, 2.0),
            Quat::IDENTITY,
            vec3(0.0, 1.0, 0.0),
        );
        assert_mat4_close(box_node.world, root_world * box_local);
        assert_vec3_close(
            box_node.world.transform_point3(vec3(1.0, 0.0, 0.0)),
            vec3(3.0, 3.0, 3.0),
        );

        let quarter_turn = Quat::from_xyzw(
            0.0,
            std::f32::consts::FRAC_1_SQRT_2,
            0.0,
            std::f32::consts::FRAC_1_SQRT_2,
        );
        assert!((spin.rotation.dot(quarter_turn).abs() - 1.0).abs() < 1e-5);
        assert_vec3_close(spin.world.transform_point3(Vec3::X), vec3(1.0, 2.0, 2.0));

        assert_vec3_close(marker.translation, vec3(4.0, 5.0, 6.0));
        assert_vec3_close(
            marker.world.transform_point3(Vec3::ZERO),
            vec3(4.0, 5.0, 6.0),
        );

        let triangle = &gltf.meshes[0].primitives[0];
        assert_eq!(triangle.positions, vec![Vec3::ZERO, Vec3::X, Vec3::Y]);
        assert_eq!(triangle.normals, vec![Vec3::Z, Vec3::Z, Vec3::Z]);
        assert_eq!(triangle.uvs, vec![Vec2::ZERO, Vec2::X, Vec2::Y]);
        assert_eq!(triangle.indices, vec![0, 1, 2]);
        assert_eq!(triangle.base_color_factor, [0.2, 0.4, 0.6, 1.0]);

        let bare = &gltf.meshes[0].primitives[1];
        assert_eq!(bare.positions, vec![Vec3::ZERO, Vec3::Z, Vec3::Y]);
        assert!(bare.normals.is_empty());
        assert!(bare.uvs.is_empty());
        assert_eq!(bare.indices, vec![0, 1, 2]);
        assert_eq!(bare.base_color_factor, [1.0, 0.0, 0.0, 0.5]);
    }

    #[test]
    fn missing_scene_and_unnamed_node() {
        let empty = GltfScene::from_bytes(br#"{"asset":{"version":"2.0"}}"#).unwrap();
        assert!(empty.nodes.is_empty());
        assert!(empty.meshes.is_empty());
        assert!(empty.colliders.is_empty());
        assert!(empty.spawns.is_empty());

        let scene = GltfScene::from_bytes(
            br#"{"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],"nodes":[{}]}"#,
        )
        .unwrap();
        assert_eq!(scene.nodes.len(), 1);
        assert_eq!(scene.nodes[0].name, None);
        assert_eq!(scene.nodes[0].parent, None);
        assert!(scene.nodes[0].children.is_empty());
        assert_eq!(scene.nodes[0].mesh, None);
        assert_eq!(scene.nodes[0].translation, Vec3::ZERO);
        assert_eq!(scene.nodes[0].rotation, Quat::IDENTITY);
        assert_eq!(scene.nodes[0].scale, Vec3::ONE);
        assert_eq!(scene.nodes[0].world, Mat4::IDENTITY);
        assert!(scene.colliders.is_empty());
        assert!(scene.spawns.is_empty());
    }

    #[test]
    fn default_scene_is_preferred_over_the_first_scene() {
        let scene = GltfScene::from_bytes(
            br#"{"asset":{"version":"2.0"},"scene":1,"scenes":[{"nodes":[0]},{"nodes":[1]}],"nodes":[{"name":"skip"},{"name":"keep"}]}"#,
        )
        .unwrap();
        assert_eq!(scene.nodes.len(), 1);
        assert_eq!(scene.nodes[0].name.as_deref(), Some("keep"));
    }

    #[test]
    fn garbage_and_external_buffers_fail() {
        assert!(GltfScene::from_bytes(b"this is not gltf").is_err());
        let external =
            br#"{"asset":{"version":"2.0"},"buffers":[{"uri":"buffer.bin","byteLength":4}]}"#;
        let error = GltfScene::from_bytes(external).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("external"), "message was {message}");
    }

    fn glb(json: &str, bin: &[u8]) -> Vec<u8> {
        fn pad(mut data: Vec<u8>, alignment: usize, byte: u8) -> Vec<u8> {
            while !data.len().is_multiple_of(alignment) {
                data.push(byte);
            }
            data
        }

        let json_chunk = pad(json.as_bytes().to_vec(), 4, b' ');
        let bin_chunk = pad(bin.to_vec(), 4, 0);
        let total = 12 + 8 + json_chunk.len() + 8 + bin_chunk.len();
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(json_chunk.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&json_chunk);
        out.extend_from_slice(&(bin_chunk.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&bin_chunk);
        out
    }

    fn error_contains(bytes: &[u8], needle: &str) {
        let message = GltfScene::from_bytes(bytes).unwrap_err().to_string();
        assert!(message.contains(needle), "message was {message}");
    }

    #[test]
    fn a_node_used_twice_or_a_cycle_is_an_error() {
        error_contains(
            br#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0,0]}],"nodes":[{"name":"a"}]}"#,
            "more than once",
        );
        error_contains(
            br#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[{"children":[1]},{"children":[0]}]}"#,
            "more than once",
        );
    }

    #[test]
    fn matrix_node_keeps_a_reflection_on_the_world_matrix() {
        let scene = GltfScene::from_bytes(
            br#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[{"name":"mirror","matrix":[-1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1]}]}"#,
        )
        .unwrap();
        let mirror = &scene.nodes[0];
        assert_vec3_close(mirror.world.transform_point3(Vec3::X), -Vec3::X);
        assert_vec3_close(mirror.world.transform_point3(Vec3::ZERO), Vec3::ZERO);
    }

    #[test]
    fn omitted_material_is_white_and_bad_primitives_fail() {
        let json = r#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[0,0,0]}],"bufferViews":[{"buffer":0,"byteLength":36}],"buffers":[{"byteLength":36}]}"#;
        let scene = GltfScene::from_bytes(&glb(json, &[0; 36])).unwrap();
        assert_eq!(
            scene.meshes[0].primitives[0].base_color_factor,
            [1.0, 1.0, 1.0, 1.0]
        );
        assert_eq!(scene.meshes[0].primitives[0].indices, vec![0, 1, 2]);

        let lines = r#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],"meshes":[{"primitives":[{"attributes":{"POSITION":0},"mode":1}]}],"accessors":[{"bufferView":0,"componentType":5126,"count":2,"type":"VEC3","min":[0,0,0],"max":[0,0,0]}],"bufferViews":[{"buffer":0,"byteLength":24}],"buffers":[{"byteLength":24}]}"#;
        error_contains(&glb(lines, &[0; 24]), "only triangles");

        // Three positions, one normal.
        let mut mismatched = vec![0u8; 48];
        mismatched[..12].fill(0);
        let mismatched_json = r#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],"meshes":[{"primitives":[{"attributes":{"POSITION":0,"NORMAL":1}}]}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[0,0,0]},{"bufferView":1,"componentType":5126,"count":1,"type":"VEC3","min":[0,0,0],"max":[0,0,0]}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":12}],"buffers":[{"byteLength":48}]}"#;
        error_contains(&glb(mismatched_json, &mismatched), "normals");

        // Indices 0, 1, 3 against 3 positions. UNSIGNED_SHORT.
        let mut indices = vec![0u8; 42];
        indices[36] = 0;
        indices[37] = 0;
        indices[38] = 1;
        indices[39] = 0;
        indices[40] = 3;
        indices[41] = 0;
        let indexed = r#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],"meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[0,0,0]},{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":6}],"buffers":[{"byteLength":42}]}"#;
        error_contains(&glb(indexed, &indices), "outside");
    }

    #[test]
    fn a_view_past_the_declared_buffer_length_is_not_read_from_padding() {
        // The chunk is long enough for 3 positions, but byteLength stops at 4.
        let json = r#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[0,0,0]}],"bufferViews":[{"buffer":0,"byteLength":36}],"buffers":[{"byteLength":4}]}"#;
        let message = GltfScene::from_bytes(&glb(json, &[0; 36]))
            .unwrap_err()
            .to_string();
        assert!(
            !message.contains("Missing data"),
            "failed before the buffer length was checked: {message}"
        );
    }

    #[test]
    fn sample_markers_are_a_box_collider_and_a_spawn() {
        let scene = GltfScene::from_bytes(include_bytes!("../examples/assets/triangle.gltf"))
            .expect("triangle.gltf");
        assert_eq!(scene.colliders.len(), 1);
        assert_eq!(scene.spawns.len(), 1);

        let collider = &scene.colliders[0];
        assert_eq!(collider.name.as_deref(), Some("crate_collider"));
        assert_eq!(collider.node, 4);
        assert!(!collider.trigger);
        let GltfColliderShape::Aabb(aabb) = collider.shape else {
            panic!("crate_collider is a box");
        };
        // Child of root at translation (1, 2, 3): local (1, 0, 0) becomes (2, 2, 3).
        assert_vec3_close(aabb.center(), vec3(2.0, 2.0, 3.0));
        assert_vec3_close(aabb.size(), vec3(2.0, 1.0, 0.5));

        let spawn = &scene.spawns[0];
        assert_eq!(spawn.name.as_deref(), Some("player_spawn"));
        assert_eq!(spawn.node, 6);
        assert_vec3_close(spawn.position, vec3(8.0, 0.0, 1.0));
    }

    fn scene_json(nodes: &str, scenes: &str) -> String {
        format!(r#"{{"asset":{{"version":"2.0"}},"scenes":[{scenes}],"nodes":[{nodes}]}}"#)
    }

    #[test]
    fn extras_override_a_suffix_and_a_spawn_ignores_shape() {
        let scene = GltfScene::from_bytes(
            scene_json(
                r#"{"name":"pad_collider","translation":[3,4,5],"extras":{"mq_role":"spawn","mq_shape":"box"}}"#,
                r#"{"nodes":[0]}"#,
            )
            .as_bytes(),
        )
        .unwrap();
        assert!(scene.colliders.is_empty());
        assert_eq!(scene.spawns.len(), 1);
        assert_eq!(scene.spawns[0].name.as_deref(), Some("pad_collider"));
        assert_vec3_close(scene.spawns[0].position, vec3(3.0, 4.0, 5.0));

        let unnamed = GltfScene::from_bytes(
            scene_json(
                r#"{"translation":[9,8,7],"extras":{"mq_role":"spawn"}}"#,
                r#"{"nodes":[0]}"#,
            )
            .as_bytes(),
        )
        .unwrap();
        assert_eq!(unnamed.spawns.len(), 1);
        assert_eq!(unnamed.spawns[0].name, None);
        assert_vec3_close(unnamed.spawns[0].position, vec3(9.0, 8.0, 7.0));
        assert!(unnamed.colliders.is_empty());
    }

    #[test]
    fn trigger_box_sphere_and_skipped_markers() {
        let trigger = GltfScene::from_bytes(
            scene_json(
                r#"{"name":"zone","translation":[0,2,0],"scale":[2,2,2],"extras":{"mq_shape":"box","mq_role":"trigger"}}"#,
                r#"{"nodes":[0]}"#,
            )
            .as_bytes(),
        )
        .unwrap();
        assert_eq!(trigger.colliders.len(), 1);
        assert!(trigger.colliders[0].trigger);
        let GltfColliderShape::Aabb(aabb) = trigger.colliders[0].shape else {
            panic!("trigger box");
        };
        assert_vec3_close(aabb.center(), vec3(0.0, 2.0, 0.0));
        assert_vec3_close(aabb.size(), vec3(2.0, 2.0, 2.0));

        let orb = GltfScene::from_bytes(
            scene_json(
                r#"{"name":"orb","translation":[1,2,3],"scale":[1,4,2],"extras":{"mq_shape":"sphere","mq_role":"trigger"}}"#,
                r#"{"nodes":[0]}"#,
            )
            .as_bytes(),
        )
        .unwrap();
        assert!(orb.colliders[0].trigger);
        let GltfColliderShape::Sphere(orb_sphere) = orb.colliders[0].shape else {
            panic!("sphere");
        };
        assert_vec3_close(orb_sphere.center, vec3(1.0, 2.0, 3.0));
        assert!((orb_sphere.radius - 4.0).abs() < 1e-5);

        let skipped = GltfScene::from_bytes(
            scene_json(
                r#"{"name":"gap_collider","scale":[3,3,3]},{"name":"pill_collider","extras":{"mq_shape":"capsule"}},{"name":"Player_Spawn","translation":[1,0,0]},{"name":"x_collider","extras":[1,2]}"#,
                r#"{"nodes":[0,1,2,3]}"#,
            )
            .as_bytes(),
        )
        .unwrap();
        assert!(skipped.colliders.is_empty());
        assert!(skipped.spawns.is_empty());
    }

    #[test]
    fn box_size_uses_world_column_lengths_including_parent_and_sign() {
        let scene = GltfScene::from_bytes(
            scene_json(
                r#"{"name":"mirror_collider","scale":[-2,4,1],"extras":{"mq_shape":"box","mq_role":"collider"}},{"scale":[-2,2,1],"children":[2]},{"name":"child_collider","translation":[1,0,0],"scale":[3,1,4],"extras":{"mq_shape":"box","mq_role":"collider"}}"#,
                r#"{"nodes":[0,1]}"#,
            )
            .as_bytes(),
        )
        .unwrap();
        assert_eq!(scene.colliders.len(), 2);

        let GltfColliderShape::Aabb(mirror) = scene.colliders[0].shape else {
            panic!("mirror");
        };
        assert_eq!(scene.colliders[0].node, 0);
        assert_vec3_close(mirror.center(), Vec3::ZERO);
        assert_vec3_close(mirror.size(), vec3(2.0, 4.0, 1.0));

        let GltfColliderShape::Aabb(child) = scene.colliders[1].shape else {
            panic!("child");
        };
        assert_eq!(scene.colliders[1].node, 2);
        assert_vec3_close(child.center(), vec3(-2.0, 0.0, 0.0));
        assert_vec3_close(child.size(), vec3(6.0, 2.0, 4.0));
    }

    #[test]
    fn a_rotated_box_keeps_its_scale_and_does_not_become_an_obb() {
        let scene = GltfScene::from_bytes(
            scene_json(
                r#"{"name":"turn_collider","translation":[1,2,3],"scale":[2,3,4],"rotation":[0,0.7071067811865476,0,0.7071067811865476],"extras":{"mq_shape":"box","mq_role":"collider"}}"#,
                r#"{"nodes":[0]}"#,
            )
            .as_bytes(),
        )
        .unwrap();
        let GltfColliderShape::Aabb(aabb) = scene.colliders[0].shape else {
            panic!("rotated box stays an AABB");
        };
        assert_vec3_close(aabb.center(), vec3(1.0, 2.0, 3.0));
        // Quarter turn about Y would swap local X and Z on a world-aligned
        // bound (size 4, 3, 2). Column lengths stay in column order.
        assert_vec3_close(aabb.size(), vec3(2.0, 3.0, 4.0));
    }

    #[test]
    fn suffix_shape_comes_from_extras_or_from_transformed_mesh_positions() {
        let positions = [
            0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 3.0, 0.0, 0.0,
            3.0,
        ];
        let mut bytes = Vec::new();
        for value in positions {
            bytes.extend(value.to_le_bytes());
        }
        let json = r#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0,2,3,4]}],"nodes":[{"translation":[0,5,0],"children":[1]},{"name":"hull_collider","translation":[10,0,0],"mesh":0},{"name":"orb_collider","translation":[4,0,0],"scale":[1,2,0.5],"mesh":0,"extras":{"mq_shape":"sphere"}},{"name":"zone_trigger","scale":[2,2,2],"mesh":0},{"name":"pill_collider","mesh":0,"extras":{"mq_shape":"capsule","mq_role":"collider"}}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}},{"attributes":{"POSITION":1}}]}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,2,0]},{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,3],"max":[0,0,3]}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":36}],"buffers":[{"byteLength":72}]}"#;
        let scene = GltfScene::from_bytes(&glb(json, &bytes)).unwrap();
        assert_eq!(scene.colliders.len(), 3);
        assert!(scene.spawns.is_empty());

        let hull = &scene.colliders[0];
        assert_eq!(hull.name.as_deref(), Some("hull_collider"));
        assert_eq!(hull.node, 1);
        assert!(!hull.trigger);
        let GltfColliderShape::Aabb(hull_aabb) = hull.shape else {
            panic!("mesh collider");
        };
        assert_vec3_close(hull_aabb.min, vec3(10.0, 5.0, 0.0));
        assert_vec3_close(hull_aabb.max, vec3(11.0, 7.0, 3.0));

        let orb = &scene.colliders[1];
        assert_eq!(orb.node, 2);
        assert!(!orb.trigger);
        let GltfColliderShape::Sphere(sphere) = orb.shape else {
            panic!("explicit sphere wins over the mesh");
        };
        assert_vec3_close(sphere.center, vec3(4.0, 0.0, 0.0));
        assert!((sphere.radius - 2.0).abs() < 1e-5);

        let zone = &scene.colliders[2];
        assert_eq!(zone.name.as_deref(), Some("zone_trigger"));
        assert!(zone.trigger);
        let GltfColliderShape::Aabb(zone_aabb) = zone.shape else {
            panic!("trigger mesh");
        };
        assert_vec3_close(zone_aabb.min, Vec3::ZERO);
        assert_vec3_close(zone_aabb.max, vec3(2.0, 4.0, 6.0));
    }

    #[test]
    fn extras_json_reads_top_level_strings_only() {
        assert_eq!(
            json_object_string(r#"{"mq_role":"spawn"}"#, "mq_role").as_deref(),
            Some("spawn")
        );
        assert_eq!(
            json_object_string(
                r#" { "note" : "a } b" , "mq_role" : "trigger" } "#,
                "mq_role"
            )
            .as_deref(),
            Some("trigger")
        );
        assert_eq!(
            json_object_string(
                r#"{"arr":[1,{"mq_role":"spawn"}],"mq_shape":"box"}"#,
                "mq_shape"
            )
            .as_deref(),
            Some("box")
        );
        assert_eq!(
            json_object_string(
                r#"{"arr":[1,{"mq_role":"spawn"}],"mq_shape":"box"}"#,
                "mq_role"
            )
            .as_deref(),
            None
        );
        assert!(json_object_string(r#"["spawn"]"#, "mq_role").is_none());
        assert!(json_object_string(r#""spawn""#, "mq_role").is_none());
        assert_eq!(
            json_object_string(r#"{"mq_shape":"bo\u0078"}"#, "mq_shape").as_deref(),
            Some("box")
        );
        assert!(json_object_string(r#"{"mq_role":1}"#, "mq_role").is_none());
    }

    #[test]
    fn nodes_outside_the_loaded_scene_are_not_markers() {
        let scene = GltfScene::from_bytes(
            scene_json(
                r#"{"name":"skip_spawn","translation":[9,0,0]},{"name":"keep_spawn","translation":[1,2,3]}"#,
                r#"{"nodes":[1]}"#,
            )
            .as_bytes(),
        )
        .unwrap();
        assert_eq!(scene.spawns.len(), 1);
        assert_eq!(scene.spawns[0].name.as_deref(), Some("keep_spawn"));
        assert_eq!(scene.spawns[0].node, 0);
        assert_vec3_close(scene.spawns[0].position, vec3(1.0, 2.0, 3.0));
    }
}
