//! glTF 2.0 scenes loaded from bytes.
//!
//! This module is behind the `gltf` feature. It parses `.glb` and `.gltf`
//! documents into plain data. It does not open a window, create GPU resources,
//! or read images. A later item turns these meshes into [`crate::models::Mesh`].
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

use std::collections::{HashMap, HashSet};

use glam::{Mat4, Quat, Vec2, Vec3};

/// A parsed glTF scene: the default scene's nodes and the meshes they use.
///
/// Nodes are stored in preorder, parents before children. A document with no
/// scenes yields empty `nodes` and `meshes`. Meshes that no loaded node
/// references are left out. Images, skins, and animations are not loaded.
#[derive(Clone, Debug, PartialEq)]
pub struct GltfScene {
    /// Nodes of the default scene, in preorder.
    pub nodes: Vec<GltfNode>,
    /// Meshes referenced by [`Self::nodes`], in the order they are first used.
    pub meshes: Vec<GltfMesh>,
}

/// One node in a [`GltfScene`].
///
/// `translation`, `rotation`, and `scale` are the node's local transform.
/// `world` is that transform composed with every ancestor. A node whose glTF
/// source is a matrix is decomposed into those three fields; `world` is built
/// from the original matrix, so it still matches the file when the matrix
/// does not decompose cleanly.
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

/// Failure while reading a glTF document from bytes.
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
    /// scene, including through a cycle, is an error.
    pub fn from_bytes(bytes: &[u8]) -> Result<GltfScene, GltfError> {
        let gltf = gltf::Gltf::from_slice(bytes).map_err(|error| {
            GltfError::new(format!("glTF document could not be parsed: {error}"))
        })?;
        let buffers = load_buffers(&gltf)?;
        let Some(scene) = gltf.default_scene().or_else(|| gltf.scenes().next()) else {
            return Ok(GltfScene {
                nodes: Vec::new(),
                meshes: Vec::new(),
            });
        };

        let mut loaded = GltfScene {
            nodes: Vec::new(),
            meshes: Vec::new(),
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
                Some("marker"),
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
        assert_eq!(root.children, vec![1, 2, 3]);
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
}
