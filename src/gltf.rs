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
//! apply a second conversion. [`GltfScene::with_up_axis`] rotates a loaded scene
//! so glTF +Y points along a caller-chosen axis; [`glam::Vec3::Y`] leaves the
//! scene unchanged. [`crate::gltf_draw::GltfModel::with_up_axis`] applies that
//! same rotation to a drawn mesh. Macroquad does not assume an up axis.
//! [`crate::camera::Camera3D`]'s default up is +Z.
//!
//! Mesh vertices stay in mesh-local space. [`GltfNode::world`] is the node's
//! place in the scene. External buffer file URIs are rejected: the bytes passed
//! to [`GltfScene::from_bytes`] must be a `.glb` or a `.gltf` whose buffers are
//! `data:` URIs. That keeps parsing free of filesystem and window access.
//! A buffer longer than its declared `byteLength` is truncated. Only triangle
//! primitives are loaded, and a node index used more than once is an error.
//!
//! [`GltfMorphData`] separately loads mesh-local POSITION/NORMAL morph deltas
//! and default weights. Its CPU evaluator applies caller weights and computes
//! deformed bounds without decoding images or opening a window. Draw models do
//! not apply these morphs yet; evaluate before the node's world/up-axis transform.
//!
//! [`GltfPrimitive::base_color_texture`] indexes [`GltfScene::images`]. Those
//! images are the base-color textures the loaded primitives sample, decoded
//! to 8-bit RGBA. An image no loaded primitive samples is left out, including
//! one named only by an external URI. Normal and metallic-roughness images,
//! their UV transforms, and tangents are retained by [`GltfMaterialMaps`]
//! instead of fields on this scene. Occlusion and emissive images are not in
//! either CPU list. An external image URI is an error when a loaded primitive
//! does sample it. Sampler wrap and filter are not stored.
//! [`crate::gltf_draw::GltfModel`] still uploads its own GPU textures.
//!
//! CPU images are decoded at their source resolution. The draw loader's
//! [`crate::gltf_draw::GltfLoadOptions::max_texture_size`] does not limit this
//! separate scene parse. Use [`GltfScene::from_bytes_with_image_limits`] to
//! reject oversized source images before decoding; its RGBA8 estimate does
//! not bound decoder scratch memory or total scene memory. If only drawing is needed, load a
//! [`crate::gltf_draw::GltfModel`] directly; parse a [`GltfScene`] as well when
//! the application needs its CPU geometry, images, node hierarchy, or markers.
//! A [`crate::gltf_draw::RetainedGltfModel`] can share the uploaded geometry
//! across draws, but its instance transform does not move this scene's markers.
//!
//! Colliders, triggers, and spawn points come from the nodes that were loaded.
//! Node `extras` are preferred: `mq_shape` is `box`, `sphere`, or `capsule`,
//! and `mq_role` is `collider`, `trigger`, or `spawn`. When `mq_role` is
//! absent, a name ending in `_collider`, `_trigger`, or `_spawn` supplies the
//! role. That ending is case-sensitive: `Crate_collider` is a collider role
//! and `Player_Spawn` is not a spawn. An empty with a role but no `mq_shape`
//! and no mesh is still not emitted as a collider. A spawn stores the world
//! translation and the node name, and ignores `mq_shape`.
//!
//! ```
//! use macroquad::gltf::GltfScene;
//!
//! let scene = GltfScene::from_bytes(
//!     br#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0,1]}],"nodes":[
//!         {"name":"Crate_collider","scale":[2.0,1.0,0.5],"extras":{"mq_shape":"box"}},
//!         {"name":"Player_Spawn","translation":[1.0,0.0,0.0]}
//!     ]}"#,
//! )
//! .unwrap();
//! assert_eq!(scene.colliders.len(), 1);
//! assert!(scene.spawns.is_empty());
//! ```
//!
//! An empty `box` or `sphere` is centered on the world-matrix translation. Its
//! size is the length of each column of that matrix's upper 3×3, so a parent's
//! scale is included and a negative scale stays positive. A `sphere` uses the
//! largest of those three lengths as the radius. A `box` whose world rotation
//! leaves the faces on the world axes, including a quarter turn, is an [`Aabb`]
//! of that world-aligned size. Any other rotation is an [`Obb`] from
//! [`Obb::from_world_matrix`]. A mesh with `mq_shape` = `box` uses the mesh's
//! local bounds in that pose, not the node scale alone: a default Blender cube
//! is size 2 (vertices at ±1), so a node scale of 1 is a box of size 2. A
//! parent scaled on one axis combined with a rotated child shears the world
//! matrix. That mesh box keeps the decomposed rotation and grows until the
//! transformed bounds sit inside it. An empty box stays the column lengths
//! and does not grow.
//!
//! A `capsule` is a [`Capsule`] along the node's world +Y (the Y column of the
//! world matrix). The radius is the larger of the world X and Z column lengths.
//! The Y column length is the distance from the center to an endpoint of the
//! inner segment. The rounded caps extend `radius` past those endpoints.
//!
//! A collider or trigger with no `mq_shape` and a mesh becomes an [`Aabb`]
//! around the mesh positions after the node's world matrix, even when that
//! node is rotated. An empty node with a role but no `mq_shape` and no mesh
//! is not a collider.

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

use glam::{Mat3, Mat4, Quat, Vec2, Vec3};

use crate::math::{Aabb, Capsule, Obb, Sphere};

pub(crate) mod image_limits;
pub(crate) mod material_maps;
mod morph;
pub use image_limits::GltfImageLimits;
pub use material_maps::{
    GltfMaterialMap, GltfMaterialMaps, GltfPrimitiveMaterialMaps, GltfUvTransform,
};
pub use morph::{
    GltfMorphData, GltfMorphEvaluation, GltfMorphMesh, GltfMorphNode, GltfMorphPrimitive,
    GltfMorphTarget,
};

/// A parsed glTF scene: the default scene's nodes, the meshes they use, and
/// the base-color images those meshes sample.
///
/// Nodes are stored in preorder, parents before children. A document with no
/// scenes yields empty `nodes`, `meshes`, and `images`. Meshes that no loaded
/// node references are left out, and so are images those meshes do not sample.
/// [`Self::images`] are decoded RGBA bytes, not GPU textures. Skins and
/// animations are not loaded. [`Self::colliders`] and [`Self::spawns`] are the
/// markers on those nodes, in the same preorder. See the module docs for the
/// markup and for which images are kept.
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
/// assert!(scene.images.is_empty());
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct GltfScene {
    /// Nodes of the default scene, in preorder.
    pub nodes: Vec<GltfNode>,
    /// Meshes referenced by [`Self::nodes`], in the order they are first used.
    pub meshes: Vec<GltfMesh>,
    /// Base-color images sampled by [`Self::meshes`], in the order they are
    /// first used.
    ///
    /// [`GltfPrimitive::base_color_texture`] indexes this list. It is not the
    /// texture index in the glTF file.
    pub images: Vec<GltfImage>,
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
/// defaulting to white when the material omits it. `base_color_texture`
/// indexes [`GltfScene::images`] when that material has a base-color image,
/// and is [`None`] otherwise. Normals and uvs, when present, have one entry
/// per position. `uvs` stay `TEXCOORD_0`; the texture index does not select
/// another UV set.
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
///     base_color_texture: None,
/// };
/// assert_eq!(primitive.indices.len(), 3);
/// assert!(primitive.base_color_texture.is_none());
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
    /// Index into [`GltfScene::images`] of the base-color texture.
    ///
    /// [`None`] when the material has no base-color image. Two primitives that
    /// sample one image share this index. See the module docs.
    pub base_color_texture: Option<usize>,
}

/// An embedded glTF image, decoded to 8-bit RGBA.
///
/// `pixels` is row-major, four bytes per pixel (R, G, B, A). Row 0 is the top
/// row, which matches glTF's top-left texture origin. The length is
/// `width * height * 4`. The channels are the values stored in the file: a
/// base-color image is sRGB, and this type does not convert them to linear.
/// JPEG has no alpha, so those pixels use alpha 255. There is no
/// [`crate::texture::Texture2D`] here.
/// Images retain their source dimensions; the optional texture cap on
/// [`crate::gltf_draw::GltfLoadOptions`] applies only to the separate draw loader.
///
/// # Example
///
/// ```
/// use macroquad::gltf::GltfScene;
///
/// let scene = GltfScene::from_bytes(include_bytes!(
///     "../examples/assets/triangle.gltf"
/// ))
/// .unwrap();
/// let image = &scene.images[0];
/// assert_eq!((image.width, image.height), (1, 1));
/// assert_eq!(image.pixels, [10, 20, 30, 255]);
/// assert_eq!(scene.meshes[0].primitives[0].base_color_texture, Some(0));
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct GltfImage {
    /// glTF image name, or [`None`] when the image has none.
    pub name: Option<String>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Row-major RGBA8 pixels. See the struct docs.
    pub pixels: Vec<u8>,
}

/// Shape produced from a glTF collider or trigger.
///
/// See the module docs for which node becomes which variant. [`Self::overlaps_sphere`]
/// and [`Self::overlaps_aabb`] test every variant, so a game does not match on
/// the shape by hand.
///
/// # Example
///
/// ```
/// use macroquad::gltf::GltfColliderShape;
/// use macroquad::math::{Aabb, Sphere, vec3};
///
/// let shape = GltfColliderShape::Aabb(Aabb::from_center_size(
///     vec3(0.0, 1.0, 0.0),
///     vec3(2.0, 1.0, 0.5),
/// ));
/// let ball = Sphere::new(vec3(0.0, 1.5, 0.0), 0.5);
/// assert!(shape.overlaps_sphere(&ball));
/// match shape {
///     GltfColliderShape::Aabb(aabb) => assert_eq!(aabb.size(), vec3(2.0, 1.0, 0.5)),
///     GltfColliderShape::Sphere(_)
///     | GltfColliderShape::Obb(_)
///     | GltfColliderShape::Capsule(_) => panic!("expected a box"),
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GltfColliderShape {
    /// Axis-aligned box. The size matches [`Aabb::from_center_size`].
    Aabb(Aabb),
    /// Sphere. The radius is the largest column length, not half of it.
    Sphere(Sphere),
    /// Oriented box. `half_extents` are half of the full size.
    Obb(Obb),
    /// Capsule. `a` and `b` are the inner segment, not the rounded hull.
    Capsule(Capsule),
}

impl GltfColliderShape {
    /// Returns true when this shape and `sphere` overlap, including a touch.
    ///
    /// A sphere sitting on a face counts. A negative or non-finite radius on
    /// either shape does not overlap, matching [`Sphere`] and [`Capsule`].
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::gltf::GltfColliderShape;
    /// use macroquad::math::{Aabb, Sphere, vec3};
    ///
    /// let floor = GltfColliderShape::Aabb(Aabb::from_center_size(
    ///     vec3(0.0, -0.1, 0.0),
    ///     vec3(4.0, 0.2, 4.0),
    /// ));
    /// let resting = Sphere::new(vec3(0.0, 0.5, 0.0), 0.5);
    /// assert!(floor.overlaps_sphere(&resting));
    /// ```
    pub fn overlaps_sphere(&self, sphere: &Sphere) -> bool {
        match *self {
            GltfColliderShape::Aabb(aabb) => sphere.overlaps_aabb(&aabb),
            GltfColliderShape::Sphere(other) => other.overlaps(sphere),
            GltfColliderShape::Obb(obb) => obb.overlaps_sphere(sphere),
            GltfColliderShape::Capsule(capsule) => capsule.overlaps_sphere(sphere),
        }
    }

    /// Returns true when this shape and `aabb` overlap, including a touch.
    ///
    /// An inverted `aabb` (`min` greater than `max` on an axis) overlaps
    /// nothing. A negative or non-finite capsule or sphere radius overlaps
    /// nothing.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::gltf::GltfColliderShape;
    /// use macroquad::math::{Aabb, Sphere, vec3};
    ///
    /// let orb = GltfColliderShape::Sphere(Sphere::new(vec3(0.0, 1.0, 0.0), 0.5));
    /// let floor = Aabb::from_center_size(vec3(0.0, 0.0, 0.0), vec3(2.0, 1.0, 2.0));
    /// assert!(orb.overlaps_aabb(&floor));
    /// ```
    pub fn overlaps_aabb(&self, aabb: &Aabb) -> bool {
        match *self {
            GltfColliderShape::Aabb(other) => other.overlaps(aabb),
            GltfColliderShape::Sphere(sphere) => sphere.overlaps_aabb(aabb),
            GltfColliderShape::Obb(obb) => obb.overlaps_aabb(aabb),
            GltfColliderShape::Capsule(capsule) => capsule.overlaps_aabb(aabb),
        }
    }
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
    /// from the nodes that load; see the module docs. Base-color images those
    /// nodes sample are decoded into [`GltfScene::images`]. An external image
    /// URI, or an image that cannot be decoded, is an error when a loaded
    /// primitive samples it.
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
    ///     GltfColliderShape::Sphere(_)
    ///     | GltfColliderShape::Obb(_)
    ///     | GltfColliderShape::Capsule(_) => panic!("expected a box"),
    /// }
    /// ```
    pub fn from_bytes(bytes: &[u8]) -> Result<GltfScene, GltfError> {
        Self::from_bytes_with_image_limits(bytes, GltfImageLimits::default())
    }

    /// Load a scene with per-image source-pixel and estimated RGBA8 byte limits.
    ///
    /// Checks every base-color image used by the selected scene before decoding
    /// any image. Other texture slots and unused images remain ignored. The
    /// limits do not bound decoder scratch memory or aggregate scene memory;
    /// prepare oversized images offline. Images retain their source dimensions.
    ///
    /// ```
    /// use macroquad::gltf::{GltfImageLimits, GltfScene};
    /// let scene = GltfScene::from_bytes_with_image_limits(
    ///     include_bytes!("../examples/assets/triangle.glb"),
    ///     GltfImageLimits {
    ///         max_source_pixels: Some(1024 * 1024),
    ///         max_decoded_bytes: Some(4 * 1024 * 1024),
    ///     },
    /// ).unwrap();
    /// assert_eq!(scene.images[0].pixels.len(), 4);
    /// ```
    pub fn from_bytes_with_image_limits(
        bytes: &[u8],
        image_limits: GltfImageLimits,
    ) -> Result<GltfScene, GltfError> {
        if bytes.starts_with(b"glTF") {
            // gltf::Gltf::from_slice owns (and copies) the complete binary
            // chunk. Parse/validate its JSON separately so large embedded
            // meshes and images remain borrowed from the caller's bytes.
            let glb = gltf::binary::Glb::from_slice(bytes).map_err(|error| {
                GltfError::new(format!("glTF document could not be parsed: {error}"))
            })?;
            let gltf = gltf::Gltf::from_slice(&glb.json).map_err(|error| {
                GltfError::new(format!("glTF document could not be parsed: {error}"))
            })?;
            Self::from_document_with_image_limits(
                &gltf.document,
                glb.bin.as_deref(),
                true,
                image_limits,
            )
        } else {
            let gltf = gltf::Gltf::from_slice(bytes).map_err(|error| {
                GltfError::new(format!("glTF document could not be parsed: {error}"))
            })?;
            Self::from_document_with_image_limits(&gltf.document, None, true, image_limits)
        }
    }

    /// Shared CPU path for an already validated document. Embedded binary data
    /// is borrowed only during construction; the returned scene owns its mesh
    /// arrays and remains independent of the source bytes. Data URIs retain
    /// their existing owned decoding behavior. GPU conversion skips CPU image
    /// retention and decodes each texture on demand with its resolution budget.
    pub(crate) fn from_document(
        document: &gltf::Document,
        binary: Option<&[u8]>,
        decode_images: bool,
    ) -> Result<GltfScene, GltfError> {
        Self::from_document_with_image_limits(
            document,
            binary,
            decode_images,
            GltfImageLimits::default(),
        )
    }

    fn from_document_with_image_limits(
        document: &gltf::Document,
        binary: Option<&[u8]>,
        decode_images: bool,
        image_limits: GltfImageLimits,
    ) -> Result<GltfScene, GltfError> {
        let buffers = load_buffers(document, binary)?;
        let Some(scene) = document
            .default_scene()
            .or_else(|| document.scenes().next())
        else {
            return Ok(GltfScene {
                nodes: Vec::new(),
                meshes: Vec::new(),
                images: Vec::new(),
                colliders: Vec::new(),
                spawns: Vec::new(),
            });
        };

        if decode_images && image_limits != GltfImageLimits::default() {
            let mut checked_images = HashSet::new();
            for mesh in used_meshes(document) {
                for primitive in mesh.primitives() {
                    let material = primitive.material();
                    if let Some(info) = material.pbr_metallic_roughness().base_color_texture() {
                        let image = info.texture().source();
                        if checked_images.insert(image.index()) {
                            let context = image_context(&image, &material, "base-color");
                            let data = image_bytes(&image, &buffers)
                                .map_err(|error| GltfError::new(format!("{context}: {error}")))?;
                            image_limits::check_image(&data, image_limits)
                                .map_err(|error| GltfError::new(format!("{context}: {error}")))?;
                        }
                    }
                }
            }
        }

        let mut loaded = GltfScene {
            nodes: Vec::new(),
            meshes: Vec::new(),
            images: Vec::new(),
            colliders: Vec::new(),
            spawns: Vec::new(),
        };
        let mut state = LoadState {
            buffers: &buffers,
            decode_images,
            mesh_of: HashMap::new(),
            image_of: HashMap::new(),
            seen_nodes: HashSet::new(),
        };
        for root in scene.nodes() {
            append_node(&mut loaded, &root, None, Mat4::IDENTITY, &mut state)?;
        }
        Ok(loaded)
    }

    /// Rotates the scene so glTF +Y points along `up`.
    ///
    /// [`Self::from_bytes`] keeps the file's Y-up axes. Blender's glTF exporter
    /// writes that Y-up file, so [`Vec3::Y`] (and any positive multiple of it)
    /// returns an equal scene. `up` is a direction: its length is ignored. A
    /// zero or non-finite `up` also returns an equal scene.
    ///
    /// Node world matrices are multiplied by that rotation. Root translation,
    /// rotation, and scale are decomposed again; a child's local transform is
    /// left as it was, so the hierarchy still composes. Mesh vertices stay in
    /// mesh-local space and move with their node's world matrix. Colliders,
    /// triggers, and spawns, which are stored in world space, are rotated by
    /// the same amount. An [`Aabb`] whose faces stay on the world axes stays
    /// an [`Aabb`]. Any other rotation of a box becomes an [`Obb`]. Images
    /// have no position, so [`GltfScene::images`] and the primitive texture
    /// indices are unchanged.
    ///
    /// [`crate::camera::Camera3D`]'s [`Default`] up axis is +Z, not +Y. A
    /// camera aimed at a scene from [`Self::from_bytes`] needs `up: Vec3::Y`,
    /// or the scene can be turned with `with_up_axis(Vec3::Z)` to match that
    /// default. This method does not change the camera default.
    /// [`crate::gltf_draw::GltfModel`] parses the file on its own. Call
    /// [`GltfModel::with_up_axis`](crate::gltf_draw::GltfModel::with_up_axis)
    /// with the same `up` so the drawn vertices match these markers.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::camera::Camera3D;
    /// use macroquad::gltf::GltfScene;
    /// use macroquad::math::{Vec3, vec3};
    ///
    /// let scene = GltfScene::from_bytes(
    ///     br#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[{"name":"player_spawn","translation":[0.0,2.0,0.0]}]}"#,
    /// )
    /// .unwrap();
    /// assert_eq!(
    ///     scene.with_up_axis(Vec3::Y).spawns[0].position,
    ///     vec3(0.0, 2.0, 0.0)
    /// );
    /// let z_up = scene.with_up_axis(Vec3::Z);
    /// assert!((z_up.spawns[0].position - vec3(0.0, 0.0, 2.0)).length() < 1.0e-4);
    /// assert_eq!(Camera3D::default().up, Vec3::Z);
    /// ```
    pub fn with_up_axis(&self, up: Vec3) -> GltfScene {
        let Some(rotation) = up_axis_rotation(up) else {
            return self.clone();
        };
        let mut scene = self.clone();
        let basis = Mat4::from_quat(rotation);
        for node in &mut scene.nodes {
            node.world = basis * node.world;
            if node.parent.is_none() {
                let (scale, rotation, translation) = node.world.to_scale_rotation_translation();
                node.scale = scale;
                node.rotation = rotation;
                node.translation = translation;
            }
        }
        for collider in &mut scene.colliders {
            collider.shape = rotate_shape(rotation, collider.shape);
        }
        for spawn in &mut scene.spawns {
            spawn.position = rotation * spawn.position;
        }
        scene
    }
}

fn load_buffers<'a>(
    document: &gltf::Document,
    mut binary: Option<&'a [u8]>,
) -> Result<Vec<Cow<'a, [u8]>>, GltfError> {
    let mut buffers = Vec::new();
    for buffer in document.buffers() {
        let data = match buffer.source() {
            gltf::buffer::Source::Bin => Cow::Borrowed(binary.take().ok_or_else(|| {
                GltfError::new(format!(
                    "glTF buffer {} needs the binary chunk, which is missing",
                    buffer.index()
                ))
            })?),
            gltf::buffer::Source::Uri(uri) => Cow::Owned(decode_data_uri(uri)?),
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
        let data = match data {
            Cow::Borrowed(data) => Cow::Borrowed(&data[..buffer.length()]),
            Cow::Owned(mut data) => {
                data.truncate(buffer.length());
                Cow::Owned(data)
            }
        };
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

struct LoadState<'a> {
    buffers: &'a [Cow<'a, [u8]>],
    decode_images: bool,
    mesh_of: HashMap<usize, usize>,
    image_of: HashMap<usize, usize>,
    seen_nodes: HashSet<usize>,
}

fn append_node(
    loaded: &mut GltfScene,
    node: &gltf::Node<'_>,
    parent: Option<usize>,
    parent_world: Mat4,
    state: &mut LoadState<'_>,
) -> Result<usize, GltfError> {
    if !state.seen_nodes.insert(node.index()) {
        return Err(GltfError::new(format!(
            "glTF node {} appears more than once in the scene graph",
            node.index()
        )));
    }
    let (translation, rotation, scale, local) = local_transform(node);
    let world = parent_world * local;
    let mesh = match node.mesh() {
        Some(mesh) => Some(mesh_index(loaded, &mesh, state)?),
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
        child_indices.push(append_node(loaded, child, Some(index), world, state)?);
    }
    loaded.nodes[index].children = child_indices;
    Ok(index)
}

pub(crate) fn local_transform(node: &gltf::Node<'_>) -> (Vec3, Quat, Vec3, Mat4) {
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
    state: &mut LoadState<'_>,
) -> Result<usize, GltfError> {
    if let Some(index) = state.mesh_of.get(&mesh.index()) {
        return Ok(*index);
    }
    let mut primitives = Vec::new();
    for (primitive_index, primitive) in mesh.primitives().enumerate() {
        primitives.push(load_primitive(
            loaded,
            mesh.index(),
            primitive_index,
            &primitive,
            state,
        )?);
    }
    let index = loaded.meshes.len();
    loaded.meshes.push(GltfMesh {
        name: mesh.name().map(str::to_owned),
        primitives,
    });
    state.mesh_of.insert(mesh.index(), index);
    Ok(index)
}

fn load_primitive(
    loaded: &mut GltfScene,
    mesh_index: usize,
    primitive_index: usize,
    primitive: &gltf::Primitive<'_>,
    state: &mut LoadState<'_>,
) -> Result<GltfPrimitive, GltfError> {
    if primitive.mode() != gltf::mesh::Mode::Triangles {
        return Err(GltfError::new(format!(
            "glTF mesh {mesh_index} primitive {primitive_index} uses {:?}; only triangles are loaded",
            primitive.mode()
        )));
    }

    let buffers = state.buffers;
    let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(|data| data.as_ref()));
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
        base_color_texture: if state.decode_images {
            base_color_image(loaded, primitive, state)?
        } else {
            None
        },
    })
}

fn base_color_image(
    loaded: &mut GltfScene,
    primitive: &gltf::Primitive<'_>,
    state: &mut LoadState<'_>,
) -> Result<Option<usize>, GltfError> {
    let Some(info) = primitive
        .material()
        .pbr_metallic_roughness()
        .base_color_texture()
    else {
        return Ok(None);
    };
    let image = info.texture().source();
    let context = image_context(&image, &primitive.material(), "base-color");
    let gltf_index = image.index();
    if let Some(index) = state.image_of.get(&gltf_index) {
        return Ok(Some(*index));
    }
    let bytes = image_bytes(&image, state.buffers)
        .map_err(|error| GltfError::new(format!("{context}: {error}")))?;
    let decoded = image::load_from_memory(&bytes)
        .map_err(|error| GltfError::new(format!("{context} could not be decoded: {error}")))?;
    let rgba = decoded.into_rgba8();
    let index = loaded.images.len();
    loaded.images.push(GltfImage {
        name: image.name().map(str::to_owned),
        width: rgba.width(),
        height: rgba.height(),
        pixels: rgba.into_raw(),
    });
    state.image_of.insert(gltf_index, index);
    Ok(Some(index))
}

/// Source meshes in the same first-use preorder as the scene loader. Repeated
/// nodes are skipped here; the scene loader reports invalid scene graphs.
pub(crate) fn used_meshes(document: &gltf::Document) -> Vec<gltf::Mesh<'_>> {
    let mut meshes = Vec::new();
    let mut seen_meshes = HashSet::new();
    let mut seen_nodes = HashSet::new();
    let mut pending = document
        .default_scene()
        .or_else(|| document.scenes().next())
        .map(|scene| scene.nodes().collect::<Vec<_>>())
        .unwrap_or_default();
    pending.reverse();
    while let Some(node) = pending.pop() {
        if !seen_nodes.insert(node.index()) {
            continue;
        }
        if let Some(mesh) = node.mesh() {
            if seen_meshes.insert(mesh.index()) {
                meshes.push(mesh);
            }
        }
        let children: Vec<_> = node.children().collect();
        pending.extend(children.into_iter().rev());
    }
    meshes
}

pub(crate) fn image_context(
    image: &gltf::Image<'_>,
    material: &gltf::Material<'_>,
    slot: &str,
) -> String {
    let image_name = image
        .name()
        .map(|name| format!(" ({name:?})"))
        .unwrap_or_default();
    let material_name = material
        .name()
        .map(|name| format!(" ({name:?})"))
        .unwrap_or_default();
    let material_index = material
        .index()
        .map(|index| index.to_string())
        .unwrap_or_else(|| "default".into());
    format!(
        "glTF image {}{image_name}, material {material_index}{material_name} {slot}",
        image.index()
    )
}

fn image_bytes<'a>(
    image: &gltf::Image<'_>,
    buffers: &'a [Cow<'_, [u8]>],
) -> Result<Cow<'a, [u8]>, GltfError> {
    match image.source() {
        gltf::image::Source::View { view, .. } => {
            let buffer_index = view.buffer().index();
            let Some(buffer) = buffers.get(buffer_index) else {
                return Err(GltfError::new(format!(
                    "glTF image {} uses buffer {buffer_index}, which is missing",
                    image.index()
                )));
            };
            let start = view.offset();
            let Some(end) = start.checked_add(view.length()) else {
                return Err(GltfError::new(format!(
                    "glTF image {} buffer view range overflows",
                    image.index()
                )));
            };
            buffer.get(start..end).map(Cow::Borrowed).ok_or_else(|| {
                GltfError::new(format!(
                    "glTF image {} buffer view is outside buffer {buffer_index}",
                    image.index()
                ))
            })
        }
        gltf::image::Source::Uri { uri, .. } => {
            decode_image_uri(uri, image.index()).map(Cow::Owned)
        }
    }
}

fn decode_image_uri(uri: &str, image_index: usize) -> Result<Vec<u8>, GltfError> {
    let Some(rest) = uri.strip_prefix("data:") else {
        return Err(GltfError::new(format!(
            "external glTF image {image_index} is not loaded from a byte slice; embed it in the buffer or as a data URI"
        )));
    };
    let Some((header, payload)) = rest.split_once(',') else {
        return Err(GltfError::new(format!(
            "glTF image {image_index} data URI has no payload"
        )));
    };
    if !header.contains("base64") {
        return Err(GltfError::new(format!(
            "glTF image {image_index} data URI is not base64"
        )));
    }
    decode_base64(payload).ok_or_else(|| {
        GltfError::new(format!(
            "glTF image {image_index} data URI is not valid base64"
        ))
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
        Some(MarkerShape::Capsule) => capsule_shape(world),
        Some(MarkerShape::Box) => box_shape(loaded, index, world),
        Some(MarkerShape::Sphere) => world_extent(world).map(|(center, size)| {
            GltfColliderShape::Sphere(Sphere::new(center, size.max_element()))
        }),
        None => loaded.nodes[index]
            .mesh
            .and_then(|mesh| mesh_aabb(loaded, mesh, world))
            .map(GltfColliderShape::Aabb),
    }
}

/// Empty boxes use `world` as the full size. A mesh box uses the local bounds
/// of that mesh, posed by `world`, so a Blender cube of size 2 is not half the
/// drawn mesh.
fn box_shape(loaded: &GltfScene, index: usize, world: Mat4) -> Option<GltfColliderShape> {
    if let Some(mesh) = loaded.nodes[index].mesh {
        let local = mesh_local_aabb(loaded, mesh)?;
        let posed = world
            * Mat4::from_scale_rotation_translation(local.size(), Quat::IDENTITY, local.center());
        let shape = shape_from_box_matrix(posed)?;
        let corners = aabb_corners(local).map(|corner| world.transform_point3(corner));
        if shape_covers_points(&shape, &corners) {
            Some(shape)
        } else {
            Some(fit_box_to_points(&shape, &corners))
        }
    } else {
        shape_from_box_matrix(world)
    }
}

fn aabb_corners(aabb: Aabb) -> [Vec3; 8] {
    let min = aabb.min;
    let max = aabb.max;
    [
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(min.x, max.y, min.z),
        Vec3::new(max.x, max.y, min.z),
        Vec3::new(min.x, min.y, max.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(min.x, max.y, max.z),
        Vec3::new(max.x, max.y, max.z),
    ]
}

fn shape_covers_points(shape: &GltfColliderShape, points: &[Vec3]) -> bool {
    points.iter().all(|point| match shape {
        GltfColliderShape::Aabb(aabb) => {
            (*point - aabb.closest_point(*point)).length_squared() <= 1.0e-6
        }
        GltfColliderShape::Obb(obb) => {
            (*point - obb.closest_point(*point)).length_squared() <= 1.0e-6
        }
        GltfColliderShape::Sphere(_) | GltfColliderShape::Capsule(_) => true,
    })
}

/// Widens `shape` in its own rotation until every point is inside.
fn fit_box_to_points(shape: &GltfColliderShape, points: &[Vec3]) -> GltfColliderShape {
    let (center, rotation) = match *shape {
        GltfColliderShape::Aabb(aabb) => (aabb.center(), Quat::IDENTITY),
        GltfColliderShape::Obb(obb) => (obb.center, obb.rotation),
        GltfColliderShape::Sphere(_) | GltfColliderShape::Capsule(_) => return *shape,
    };
    let rotation = if rotation.length_squared() > f32::EPSILON {
        rotation.normalize()
    } else {
        Quat::IDENTITY
    };
    let inverse = rotation.inverse();
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for point in points {
        let local = inverse * (*point - center);
        min = min.min(local);
        max = max.max(local);
    }
    if !min.is_finite() || !max.is_finite() {
        return *shape;
    }
    let mid = (min + max) * 0.5;
    let half = (max - min) * 0.5;
    let fitted = Obb::new(center + rotation * mid, half, rotation);
    if rotation_is_axis_aligned(rotation) {
        GltfColliderShape::Aabb(aabb_from_aligned_obb(fitted))
    } else {
        GltfColliderShape::Obb(fitted)
    }
}

/// An axis-aligned rotation, including a quarter turn, is an [`Aabb`] whose
/// size lies on the world axes. Any other rotation stays an [`Obb`].
fn shape_from_box_matrix(posed: Mat4) -> Option<GltfColliderShape> {
    let obb = Obb::from_world_matrix(posed);
    if !obb.center.is_finite() || !obb.half_extents.is_finite() {
        return None;
    }
    if rotation_is_axis_aligned(obb.rotation) {
        Some(GltfColliderShape::Aabb(aabb_from_aligned_obb(obb)))
    } else {
        Some(GltfColliderShape::Obb(obb))
    }
}

fn rotation_is_axis_aligned(rotation: Quat) -> bool {
    if !rotation.is_finite() || rotation.length_squared() <= f32::EPSILON {
        return false;
    }
    let matrix = Mat3::from_quat(rotation.normalize());
    [matrix.x_axis, matrix.y_axis, matrix.z_axis]
        .into_iter()
        .all(|column| {
            let abs = column.abs();
            let dominant = abs.max_element();
            dominant >= 1.0 - AXIS_ALIGN_EPSILON
                && abs.x + abs.y + abs.z - dominant <= AXIS_ALIGN_EPSILON
        })
}

fn aabb_from_aligned_obb(obb: Obb) -> Aabb {
    let matrix = Mat3::from_quat(obb.rotation.normalize());
    let full = obb.half_extents * 2.0;
    let size =
        matrix.x_axis.abs() * full.x + matrix.y_axis.abs() * full.y + matrix.z_axis.abs() * full.z;
    Aabb::from_center_size(obb.center, size)
}

/// Capsule along the world matrix's Y column.
///
/// That column's length is the distance from the center to one inner endpoint.
/// The radius is the larger of the X and Z column lengths.
fn capsule_shape(world: Mat4) -> Option<GltfColliderShape> {
    let center = world.transform_point3(Vec3::ZERO);
    let x_len = world.x_axis.truncate().length();
    let y_axis = world.y_axis.truncate();
    let y_len = y_axis.length();
    let z_len = world.z_axis.truncate().length();
    if !center.is_finite() || !x_len.is_finite() || !y_len.is_finite() || !z_len.is_finite() {
        return None;
    }
    Some(GltfColliderShape::Capsule(Capsule::new(
        center - y_axis,
        center + y_axis,
        x_len.max(z_len),
    )))
}

const AXIS_ALIGN_EPSILON: f32 = 1.0e-3;

/// Rotation that carries glTF +Y onto `up`.
///
/// [`None`] when the data should stay as loaded: `up` is zero or not finite,
/// `up` already points along +Y (any positive scale), or the quaternion is
/// within `1e-5` of identity. [`GltfScene::with_up_axis`] and
/// [`crate::gltf_draw::GltfModel::with_up_axis`] both treat [`None`] as
/// "return a copy".
pub(crate) fn up_axis_rotation(up: Vec3) -> Option<Quat> {
    if !up.is_finite() {
        return None;
    }
    let length_sq = up.length_squared();
    if length_sq <= f32::EPSILON {
        return None;
    }
    let up = up / length_sq.sqrt();
    if !up.is_finite() || (up - Vec3::Y).length_squared() <= 1.0e-12 {
        return None;
    }
    let rotation = Quat::from_rotation_arc(Vec3::Y, up);
    if !rotation.is_finite() || rotation.dot(Quat::IDENTITY).abs() > 1.0 - 1.0e-5 {
        return None;
    }
    Some(rotation)
}

fn rotate_shape(rotation: Quat, shape: GltfColliderShape) -> GltfColliderShape {
    match shape {
        GltfColliderShape::Sphere(sphere) => {
            GltfColliderShape::Sphere(Sphere::new(rotation * sphere.center, sphere.radius))
        }
        GltfColliderShape::Capsule(capsule) => GltfColliderShape::Capsule(Capsule::new(
            rotation * capsule.a,
            rotation * capsule.b,
            capsule.radius,
        )),
        GltfColliderShape::Obb(obb) => GltfColliderShape::Obb(Obb::new(
            rotation * obb.center,
            obb.half_extents,
            rotation * obb.rotation,
        )),
        GltfColliderShape::Aabb(aabb) => {
            let turned = Obb::new(rotation * aabb.center(), aabb.half_extents(), rotation);
            if rotation_is_axis_aligned(rotation) {
                GltfColliderShape::Aabb(aabb_from_aligned_obb(turned))
            } else {
                GltfColliderShape::Obb(turned)
            }
        }
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

fn mesh_local_aabb(loaded: &GltfScene, mesh: usize) -> Option<Aabb> {
    let mut points = Vec::new();
    for primitive in &loaded.meshes[mesh].primitives {
        for position in &primitive.positions {
            if !position.is_finite() {
                return None;
            }
            points.push(*position);
        }
    }
    Aabb::from_points(points)
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

    #[test]
    fn embedded_buffers_borrow_source_and_data_uris_stay_owned() {
        let parsed = gltf::Gltf::from_slice(
            br#"{
            "asset":{"version":"2.0"},
            "buffers":[
                {"byteLength":3},
                {"uri":"data:application/octet-stream;base64,BAUGBw==","byteLength":2}
            ]
        }"#,
        )
        .unwrap();
        let binary = [1, 2, 3, 0];
        let buffers = load_buffers(&parsed.document, Some(&binary)).unwrap();
        assert!(matches!(&buffers[0], Cow::Borrowed(bytes) if bytes.as_ptr() == binary.as_ptr()));
        assert_eq!(buffers[0].as_ref(), &[1, 2, 3]);
        assert!(matches!(&buffers[1], Cow::Owned(_)));
        assert_eq!(buffers[1].as_ref(), &[4, 5]);
        assert!(load_buffers(&parsed.document, None).is_err());
        assert!(load_buffers(&parsed.document, Some(&binary[..2])).is_err());
    }

    #[test]
    fn validated_borrowed_document_matches_public_glb_loader() {
        let bytes = include_bytes!("../examples/assets/triangle.glb");
        let glb = gltf::binary::Glb::from_slice(bytes).unwrap();
        let parsed = gltf::Gltf::from_slice(&glb.json).unwrap();
        let scene = GltfScene::from_document(&parsed.document, glb.bin.as_deref(), true).unwrap();
        assert_eq!(scene, GltfScene::from_bytes(bytes).unwrap());
    }
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
        assert_eq!(triangle.base_color_texture, Some(0));
        assert_eq!(gltf.images.len(), 1);
        assert_eq!(gltf.images[0].name.as_deref(), Some("pixel"));
        assert_eq!((gltf.images[0].width, gltf.images[0].height), (1, 1));
        assert_eq!(gltf.images[0].pixels, [10, 20, 30, 255]);

        let bare = &gltf.meshes[0].primitives[1];
        assert_eq!(bare.positions, vec![Vec3::ZERO, Vec3::Z, Vec3::Y]);
        assert!(bare.normals.is_empty());
        assert!(bare.uvs.is_empty());
        assert_eq!(bare.indices, vec![0, 1, 2]);
        assert_eq!(bare.base_color_factor, [1.0, 0.0, 0.0, 0.5]);
        assert_eq!(bare.base_color_texture, None);
    }

    #[test]
    fn missing_scene_and_unnamed_node() {
        let empty = GltfScene::from_bytes(br#"{"asset":{"version":"2.0"}}"#).unwrap();
        assert!(empty.nodes.is_empty());
        assert!(empty.meshes.is_empty());
        assert!(empty.images.is_empty());
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
        assert_eq!(scene.meshes[0].primitives[0].base_color_texture, None);
        assert!(scene.images.is_empty());
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

    fn rgba_png(pixels: &[u8]) -> Vec<u8> {
        use image::ImageEncoder;
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(pixels, 1, 1, image::ColorType::Rgba8)
            .expect("png");
        png
    }

    fn base64_encode(bytes: &[u8]) -> String {
        const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        let mut index = 0;
        while index + 3 <= bytes.len() {
            let triple = ((bytes[index] as u32) << 16)
                | ((bytes[index + 1] as u32) << 8)
                | bytes[index + 2] as u32;
            out.push(TABLE[((triple >> 18) & 63) as usize] as char);
            out.push(TABLE[((triple >> 12) & 63) as usize] as char);
            out.push(TABLE[((triple >> 6) & 63) as usize] as char);
            out.push(TABLE[(triple & 63) as usize] as char);
            index += 3;
        }
        let rest = bytes.len() - index;
        if rest == 1 {
            let triple = (bytes[index] as u32) << 16;
            out.push(TABLE[((triple >> 18) & 63) as usize] as char);
            out.push(TABLE[((triple >> 12) & 63) as usize] as char);
            out.push('=');
            out.push('=');
        } else if rest == 2 {
            let triple = ((bytes[index] as u32) << 16) | ((bytes[index + 1] as u32) << 8);
            out.push(TABLE[((triple >> 18) & 63) as usize] as char);
            out.push(TABLE[((triple >> 12) & 63) as usize] as char);
            out.push(TABLE[((triple >> 6) & 63) as usize] as char);
            out.push('=');
        }
        out
    }

    fn positions_json(extra: &str) -> String {
        format!(
            r#"{{"asset":{{"version":"2.0"}},"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],{extra},"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[0,0,0]}}],"bufferViews":[{{"buffer":0,"byteLength":36}}],"buffers":[{{"byteLength":36}}]}}"#
        )
    }

    #[test]
    fn base_color_images_are_shared_rgba_bytes() {
        let png = rgba_png(&[10, 20, 30, 40]);
        let mut bin = vec![0u8; 36];
        bin.extend_from_slice(&png);
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"material":0}},{{"attributes":{{"POSITION":0}},"material":0}},{{"attributes":{{"POSITION":0}},"material":1}}]}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[0.2,0.4,0.6,1],"baseColorTexture":{{"index":0}}}}}},{{"pbrMetallicRoughness":{{"baseColorFactor":[0,1,0,1]}}}}],"textures":[{{"source":0}}],"images":[{{"name":"dot","mimeType":"image/png","bufferView":1}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[0,0,0]}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":{}}}],"buffers":[{{"byteLength":{}}}]}}"#,
            png.len(),
            bin.len()
        );
        let scene = GltfScene::from_bytes(&glb(&json, &bin)).unwrap();
        assert_eq!(scene.images.len(), 1);
        assert_eq!(scene.images[0].name.as_deref(), Some("dot"));
        assert_eq!((scene.images[0].width, scene.images[0].height), (1, 1));
        assert_eq!(scene.images[0].pixels, [10, 20, 30, 40]);
        assert_eq!(scene.meshes[0].primitives[0].base_color_texture, Some(0));
        assert_eq!(scene.meshes[0].primitives[1].base_color_texture, Some(0));
        assert_eq!(scene.meshes[0].primitives[2].base_color_texture, None);
        assert_eq!(
            scene.meshes[0].primitives[0].base_color_factor,
            [0.2, 0.4, 0.6, 1.0]
        );

        let uri = format!("data:image/png;base64,{}", base64_encode(&png));
        let data_uri = format!(
            r#"{{"asset":{{"version":"2.0"}},"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"material":0}}]}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}}}}],"textures":[{{"source":0}}],"images":[{{"uri":"{uri}"}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[0,0,0]}}],"bufferViews":[{{"buffer":0,"byteLength":36}}],"buffers":[{{"byteLength":36}}]}}"#
        );
        let from_uri = GltfScene::from_bytes(&glb(&data_uri, &[0; 36])).unwrap();
        assert_eq!(from_uri.images[0].pixels, [10, 20, 30, 40]);
        assert_eq!(from_uri.images[0].name, None);
    }

    #[test]
    fn external_and_undecodable_images_fail_only_when_sampled() {
        let external = positions_json(
            r#""meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":0}]}],"materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}],"textures":[{"source":0}],"images":[{"uri":"pixel.png"}]"#,
        );
        error_contains(&glb(&external, &[0; 36]), "external");

        let unused = positions_json(
            r#""meshes":[{"primitives":[{"attributes":{"POSITION":0}}]},{"primitives":[{"attributes":{"POSITION":0},"material":0}]}],"materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}],"textures":[{"source":0}],"images":[{"uri":"pixel.png"}]"#,
        );
        let scene = GltfScene::from_bytes(&glb(&unused, &[0; 36])).unwrap();
        assert!(scene.images.is_empty());
        assert_eq!(scene.meshes.len(), 1);
        assert_eq!(scene.meshes[0].primitives[0].base_color_texture, None);

        let mut bin = vec![0u8; 36];
        bin.extend_from_slice(b"nope");
        let corrupt = r#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],"meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":0}]}],"materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}],"textures":[{"source":0}],"images":[{"mimeType":"image/png","bufferView":1}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[0,0,0]}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":4}],"buffers":[{"byteLength":40}]}"#;
        error_contains(&glb(corrupt, &bin), "could not be decoded");

        let past_end = r#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],"meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":0}]}],"materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}],"textures":[{"source":0}],"images":[{"mimeType":"image/png","bufferView":1}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[0,0,0]}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":8}],"buffers":[{"byteLength":36}]}"#;
        error_contains(&glb(past_end, &[0; 36]), "outside");
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
        assert_eq!(skipped.colliders.len(), 1);
        assert_eq!(skipped.colliders[0].name.as_deref(), Some("pill_collider"));
        assert!(!skipped.colliders[0].trigger);
        let GltfColliderShape::Capsule(pill) = skipped.colliders[0].shape else {
            panic!("capsule empty");
        };
        // Identity scale: radius 1, and the Y column reaches 1 on each side.
        assert_vec3_close(pill.a, vec3(0.0, -1.0, 0.0));
        assert_vec3_close(pill.b, vec3(0.0, 1.0, 0.0));
        assert!((pill.radius - 1.0).abs() < 1e-5);
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
    fn a_quarter_turn_box_stays_an_aabb_and_a_yaw_is_an_obb() {
        let scene = GltfScene::from_bytes(
            scene_json(
                r#"{"name":"turn_collider","translation":[1,2,3],"scale":[2,3,4],"rotation":[0,0.7071067811865476,0,0.7071067811865476],"extras":{"mq_shape":"box","mq_role":"collider"}}"#,
                r#"{"nodes":[0]}"#,
            )
            .as_bytes(),
        )
        .unwrap();
        let GltfColliderShape::Aabb(aabb) = scene.colliders[0].shape else {
            panic!("quarter turn stays an AABB");
        };
        let center = vec3(1.0, 2.0, 3.0);
        assert_vec3_close(aabb.center(), center);
        // 90° about Y maps local X (size 2) onto world Z and local Z (size 4)
        // onto world X. The faces are still on the world axes.
        assert_vec3_close(aabb.size(), vec3(4.0, 3.0, 2.0));
        assert!(aabb.contains(center + vec3(2.0, 0.0, 0.0)));
        assert!(!aabb.contains(center + vec3(2.2, 0.0, 0.0)));
        assert!(aabb.contains(center + vec3(0.0, 0.0, -1.0)));
        assert!(!aabb.contains(center + vec3(0.0, 0.0, -1.2)));

        let yaw = Quat::from_rotation_y(45.0_f32.to_radians());
        let yawed = GltfScene::from_bytes(
            scene_json(
                &format!(
                    r#"{{"name":"yaw_collider","translation":[1,2,3],"scale":[2,3,4],"rotation":[{},{},{},{}],"extras":{{"mq_shape":"box","mq_role":"collider"}}}},{{"rotation":[{},{},{},{}],"children":[2]}},{{"name":"child_collider","scale":[2,2,2],"extras":{{"mq_shape":"box","mq_role":"collider"}}}}"#,
                    yaw.x, yaw.y, yaw.z, yaw.w, yaw.x, yaw.y, yaw.z, yaw.w
                ),
                r#"{"nodes":[0,1]}"#,
            )
            .as_bytes(),
        )
        .unwrap();
        let GltfColliderShape::Obb(obb) = yawed.colliders[0].shape else {
            panic!("45° box is an Obb");
        };
        assert_vec3_close(obb.center, center);
        assert_vec3_close(obb.half_extents, vec3(1.0, 1.5, 2.0));
        assert!(obb.rotation.dot(yaw).abs() > 1.0 - 1e-4);
        let on_face = Sphere::new(obb.center + obb.rotation * vec3(1.0, 0.0, 0.0), 0.0);
        assert!(yawed.colliders[0].shape.overlaps_sphere(&on_face));
        let clear = Sphere::new(obb.center + obb.rotation * vec3(1.5, 0.0, 0.0), 0.1);
        assert!(!yawed.colliders[0].shape.overlaps_sphere(&clear));

        let GltfColliderShape::Obb(child) = yawed.colliders[1].shape else {
            panic!("a child of a yawed parent is an Obb");
        };
        assert_eq!(yawed.colliders[1].node, 2);
        assert_vec3_close(child.half_extents, Vec3::ONE);
        assert!(child.rotation.dot(yaw).abs() > 1.0 - 1e-4);
    }

    #[test]
    fn capsule_empty_uses_world_y_and_the_wider_horizontal_scale() {
        let turn = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let scene = GltfScene::from_bytes(
            scene_json(
                &format!(
                    r#"{{"name":"pill_trigger","translation":[1,2,3],"scale":[0.4,1.5,0.25],"extras":{{"mq_shape":"capsule","mq_role":"trigger"}}}},{{"name":"lean_collider","translation":[1,2,3],"scale":[0.4,1.5,0.25],"rotation":[{},{},{},{}],"extras":{{"mq_shape":"capsule","mq_role":"collider"}}}},{{"scale":[2,3,4],"children":[3]}},{{"name":"child_collider","extras":{{"mq_shape":"capsule","mq_role":"collider"}}}}"#,
                    turn.x, turn.y, turn.z, turn.w
                ),
                r#"{"nodes":[0,1,2]}"#,
            )
            .as_bytes(),
        )
        .unwrap();
        assert_eq!(scene.colliders.len(), 3);

        assert!(scene.colliders[0].trigger);
        let GltfColliderShape::Capsule(upright) = scene.colliders[0].shape else {
            panic!("capsule");
        };
        assert_vec3_close(upright.a, vec3(1.0, 0.5, 3.0));
        assert_vec3_close(upright.b, vec3(1.0, 3.5, 3.0));
        assert!((upright.radius - 0.4).abs() < 1e-5);
        let floor = Aabb::from_center_size(vec3(1.0, 0.5, 3.0), vec3(0.8, 0.0, 0.8));
        assert!(scene.colliders[0].shape.overlaps_aabb(&floor));

        let GltfColliderShape::Capsule(leaning) = scene.colliders[1].shape else {
            panic!("rotated capsule");
        };
        // 90° about Z sends local +Y to world -X, so the Y column is (-1.5, 0, 0).
        assert_vec3_close(leaning.a, vec3(2.5, 2.0, 3.0));
        assert_vec3_close(leaning.b, vec3(-0.5, 2.0, 3.0));
        assert!((leaning.radius - 0.4).abs() < 1e-5);

        let GltfColliderShape::Capsule(child) = scene.colliders[2].shape else {
            panic!("child capsule");
        };
        assert_eq!(scene.colliders[2].node, 3);
        assert_vec3_close(child.a, vec3(0.0, -3.0, 0.0));
        assert_vec3_close(child.b, vec3(0.0, 3.0, 0.0));
        assert!((child.radius - 4.0).abs() < 1e-5);
    }

    #[test]
    fn a_mesh_box_uses_vertex_bounds_not_the_node_scale_alone() {
        let yaw = Quat::from_rotation_y(45.0_f32.to_radians());
        let default_cube = mesh_marker_glb(
            &format!(
                r#"{{"name":"cube_collider","translation":[4,0,0],"rotation":[{},{},{},{}],"mesh":0,"extras":{{"mq_shape":"box","mq_role":"collider"}}}}"#,
                yaw.x, yaw.y, yaw.z, yaw.w
            ),
            &[
                vec3(-1.0, -1.0, -1.0),
                vec3(1.0, 1.0, 1.0),
                vec3(1.0, -1.0, 1.0),
            ],
        );
        let scene = GltfScene::from_bytes(&default_cube).unwrap();
        let GltfColliderShape::Obb(obb) = scene.colliders[0].shape else {
            panic!("yawed mesh box is an Obb");
        };
        // Vertices at ±1 and node scale 1. `from_world_matrix` on the node
        // alone would use half extents 0.5.
        assert_vec3_close(obb.center, vec3(4.0, 0.0, 0.0));
        assert_vec3_close(obb.half_extents, Vec3::ONE);
        let on_face = obb.center + obb.rotation * Vec3::X;
        assert!(obb.contains(on_face));
        assert!(!obb.contains(obb.center + obb.rotation * vec3(1.2, 0.0, 0.0)));

        let offset = mesh_marker_glb(
            r#"{"name":"offset_collider","translation":[10,0,0],"mesh":0,"extras":{"mq_shape":"box","mq_role":"collider"}}"#,
            &[
                vec3(0.0, 0.0, 0.0),
                vec3(2.0, 0.0, 0.0),
                vec3(0.0, 2.0, 2.0),
            ],
        );
        let offset = GltfScene::from_bytes(&offset).unwrap();
        let GltfColliderShape::Aabb(aabb) = offset.colliders[0].shape else {
            panic!("axis-aligned mesh box stays an AABB");
        };
        assert_vec3_close(aabb.center(), vec3(11.0, 1.0, 1.0));
        assert_vec3_close(aabb.size(), vec3(2.0, 2.0, 2.0));

        let unit = mesh_marker_glb(
            r#"{"name":"unit_collider","translation":[0,1,0],"scale":[2,4,6],"mesh":0,"extras":{"mq_shape":"box","mq_role":"collider"}}"#,
            &[
                vec3(-0.5, -0.5, -0.5),
                vec3(0.5, 0.5, 0.5),
                vec3(0.5, -0.5, 0.5),
            ],
        );
        let unit = GltfScene::from_bytes(&unit).unwrap();
        let GltfColliderShape::Aabb(unit_aabb) = unit.colliders[0].shape else {
            panic!("size-1 cube");
        };
        // Vertices at ±0.5, so the node scale is the full size.
        assert_vec3_close(unit_aabb.center(), vec3(0.0, 1.0, 0.0));
        assert_vec3_close(unit_aabb.size(), vec3(2.0, 4.0, 6.0));
    }

    #[test]
    fn a_sheared_mesh_box_contains_the_transformed_corners() {
        let yaw = Quat::from_rotation_y(45.0_f32.to_radians());
        let corners = [
            vec3(-1.0, -1.0, -1.0),
            vec3(1.0, -1.0, -1.0),
            vec3(-1.0, 1.0, -1.0),
            vec3(1.0, 1.0, -1.0),
            vec3(-1.0, -1.0, 1.0),
            vec3(1.0, -1.0, 1.0),
            vec3(-1.0, 1.0, 1.0),
            vec3(1.0, 1.0, 1.0),
        ];
        let node = format!(
            r#"{{"scale":[2,1,1],"children":[1]}},{{"name":"cube_collider","rotation":[{},{},{},{}],"mesh":0,"extras":{{"mq_shape":"box","mq_role":"collider"}}}}"#,
            yaw.x, yaw.y, yaw.z, yaw.w
        );
        let mut bytes = Vec::new();
        // A ninth copy keeps the index count a multiple of 3. The bounds stay
        // the cube.
        for position in corners.iter().copied().chain(std::iter::once(corners[0])) {
            for value in [position.x, position.y, position.z] {
                bytes.extend(value.to_le_bytes());
            }
        }
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scenes":[{{"nodes":[0]}}],"nodes":[{node}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}}}}]}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":9,"type":"VEC3","min":[-1,-1,-1],"max":[1,1,1]}}],"bufferViews":[{{"buffer":0,"byteLength":{}}}],"buffers":[{{"byteLength":{}}}]}}"#,
            bytes.len(),
            bytes.len()
        );
        let scene = GltfScene::from_bytes(&glb(&json, &bytes)).unwrap();
        let world = scene.nodes[1].world;
        let GltfColliderShape::Obb(obb) = scene.colliders[0].shape else {
            panic!("sheared yaw stays an Obb");
        };
        for corner in corners {
            let placed = world.transform_point3(corner);
            assert!(obb.contains(placed), "corner {placed:?} is outside {obb:?}");
        }
    }

    #[test]
    fn negative_scale_root_still_composes_after_up_axis() {
        let scene = GltfScene::from_bytes(
            scene_json(
                r#"{"name":"root","scale":[-2,3,4],"children":[1]},{"name":"child_spawn","translation":[1,0,0]}"#,
                r#"{"nodes":[0]}"#,
            )
            .as_bytes(),
        )
        .unwrap();
        let turned = scene.with_up_axis(Vec3::Z);
        let root = &turned.nodes[0];
        let rebuilt =
            Mat4::from_scale_rotation_translation(root.scale, root.rotation, root.translation);
        assert_mat4_close(rebuilt, root.world);
        let child = &turned.nodes[1];
        let child_local =
            Mat4::from_scale_rotation_translation(child.scale, child.rotation, child.translation);
        assert_mat4_close(root.world * child_local, child.world);
        assert_eq!(turned.spawns.len(), 1);
        assert_vec3_close(
            turned.spawns[0].position,
            child.world.transform_point3(Vec3::ZERO),
        );
    }

    fn mesh_marker_glb(node: &str, positions: &[Vec3]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for position in positions {
            for value in [position.x, position.y, position.z] {
                bytes.extend(value.to_le_bytes());
            }
        }
        let mut min = positions[0];
        let mut max = positions[0];
        for position in positions {
            min = min.min(*position);
            max = max.max(*position);
        }
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scenes":[{{"nodes":[0]}}],"nodes":[{node}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}}}}]}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":{},"type":"VEC3","min":[{},{},{}],"max":[{},{},{}]}}],"bufferViews":[{{"buffer":0,"byteLength":{}}}],"buffers":[{{"byteLength":{}}}]}}"#,
            positions.len(),
            min.x,
            min.y,
            min.z,
            max.x,
            max.y,
            max.z,
            bytes.len(),
            bytes.len()
        );
        glb(&json, &bytes)
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
        assert_eq!(scene.colliders.len(), 4);
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

        // `mq_shape = capsule` ignores the mesh and uses the world matrix.
        let pill = &scene.colliders[3];
        assert_eq!(pill.name.as_deref(), Some("pill_collider"));
        assert_eq!(pill.node, 4);
        let GltfColliderShape::Capsule(capsule) = pill.shape else {
            panic!("capsule wins over the mesh");
        };
        assert_vec3_close(capsule.a, vec3(0.0, -1.0, 0.0));
        assert_vec3_close(capsule.b, vec3(0.0, 1.0, 0.0));
        assert!((capsule.radius - 1.0).abs() < 1e-5);
    }

    #[test]
    fn collider_shapes_overlap_a_touching_sphere_and_aabb() {
        let floor = GltfColliderShape::Aabb(Aabb::from_center_size(
            vec3(0.0, -0.1, 0.0),
            vec3(4.0, 0.2, 4.0),
        ));
        let resting = Sphere::new(vec3(0.0, 0.5, 0.0), 0.5);
        assert!(floor.overlaps_sphere(&resting));
        assert!(!floor.overlaps_sphere(&Sphere::new(vec3(0.0, 1.0, 0.0), 0.4)));

        let orb = GltfColliderShape::Sphere(Sphere::new(vec3(0.0, 1.0, 0.0), 1.0));
        let pad = Aabb::from_center_size(vec3(0.0, 0.0, 0.0), vec3(2.0, 0.0, 2.0));
        assert!(orb.overlaps_aabb(&pad));

        let obb =
            GltfColliderShape::Obb(Obb::new(Vec3::ZERO, Vec3::ONE, Quat::from_rotation_y(0.4)));
        assert!(obb.overlaps_aabb(&Aabb::from_center_size(Vec3::ZERO, Vec3::splat(0.2))));
        assert!(obb.overlaps_sphere(&Sphere::new(vec3(1.2, 0.0, 0.0), 0.3)));

        let body = GltfColliderShape::Capsule(Capsule::new(Vec3::ZERO, vec3(0.0, 2.0, 0.0), 0.5));
        assert!(body.overlaps_aabb(&Aabb::from_min_max(
            vec3(-1.0, -0.5, -1.0),
            vec3(1.0, 0.0, 1.0),
        )));
        assert!(body.overlaps_sphere(&Sphere::new(vec3(0.5, 1.0, 0.0), 0.0)));
        assert!(!body.overlaps_sphere(&Sphere::new(vec3(2.0, 1.0, 0.0), 0.4)));
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

    #[test]
    fn y_up_and_an_unusable_up_leave_the_scene_unchanged() {
        let scene = GltfScene::from_bytes(include_bytes!("../examples/assets/triangle.gltf"))
            .expect("triangle.gltf");
        assert_eq!(scene.with_up_axis(Vec3::Y), scene);
        assert_eq!(scene.with_up_axis(Vec3::Y * 4.0), scene);
        assert_eq!(scene.with_up_axis(Vec3::ZERO), scene);
        assert_eq!(scene.with_up_axis(Vec3::NAN), scene);
    }

    #[test]
    fn z_up_maps_gltf_y_onto_z_and_moves_markers_with_meshes() {
        let positions = [0.0f32, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let mut bytes = Vec::new();
        for value in positions {
            bytes.extend(value.to_le_bytes());
        }
        let json = r#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0,2,3,4,5]}],"nodes":[{"name":"root","translation":[0,5,0],"children":[1]},{"name":"floor_collider","translation":[1,0,0],"scale":[2,0.2,4],"extras":{"mq_shape":"box","mq_role":"collider"}},{"name":"player_spawn","translation":[0,4,1]},{"name":"orb","translation":[0,1,0],"scale":[0.5,0.5,0.5],"mesh":0,"extras":{"mq_shape":"sphere","mq_role":"collider"}},{"name":"ramp_collider","translation":[2,0,0],"rotation":[0,0.3826834323650898,0,0.9238795325112867],"scale":[2,1,0.4],"extras":{"mq_shape":"box"}},{"name":"pill_collider","translation":[0,3,0],"scale":[0.25,1.5,0.1],"extras":{"mq_shape":"capsule","mq_role":"trigger"}}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,1]}],"bufferViews":[{"buffer":0,"byteLength":36}],"buffers":[{"byteLength":36}]}"#;
        let scene = GltfScene::from_bytes(&glb(json, &bytes)).unwrap();
        // The loader itself stays Y-up.
        assert_vec3_close(scene.spawns[0].position, vec3(0.0, 4.0, 1.0));

        let turn = Quat::from_rotation_arc(Vec3::Y, Vec3::Z);
        assert_vec3_close(turn * Vec3::Y, Vec3::Z);
        let z_up = scene.with_up_axis(Vec3::Z);

        let root = &z_up.nodes[0];
        let floor = &z_up.nodes[1];
        assert_eq!(floor.parent, Some(0));
        assert_eq!(floor.translation, scene.nodes[1].translation);
        assert_vec3_close(root.translation, turn * scene.nodes[0].translation);
        let floor_local =
            Mat4::from_scale_rotation_translation(floor.scale, floor.rotation, floor.translation);
        assert_mat4_close(root.world * floor_local, floor.world);

        let mesh = &z_up.nodes[3];
        let local_y = vec3(0.0, 1.0, 0.0);
        assert_vec3_close(
            mesh.world.transform_point3(local_y),
            turn * scene.nodes[3].world.transform_point3(local_y),
        );
        let y_column = mesh.world.y_axis.truncate();
        assert_vec3_close(y_column.normalize(), Vec3::Z);

        let GltfColliderShape::Aabb(floor_box) = z_up.colliders[0].shape else {
            panic!("quarter turn keeps the floor an Aabb");
        };
        let GltfColliderShape::Aabb(old_floor) = scene.colliders[0].shape else {
            panic!("floor starts axis-aligned");
        };
        let expected = Aabb::from_points(
            [
                old_floor.min,
                vec3(old_floor.max.x, old_floor.min.y, old_floor.min.z),
                vec3(old_floor.min.x, old_floor.max.y, old_floor.min.z),
                vec3(old_floor.min.x, old_floor.min.y, old_floor.max.z),
                vec3(old_floor.max.x, old_floor.max.y, old_floor.min.z),
                vec3(old_floor.max.x, old_floor.min.y, old_floor.max.z),
                vec3(old_floor.min.x, old_floor.max.y, old_floor.max.z),
                old_floor.max,
            ]
            .into_iter()
            .map(|corner| turn * corner),
        )
        .unwrap();
        assert_vec3_close(floor_box.min, expected.min);
        assert_vec3_close(floor_box.max, expected.max);
        assert_vec3_close(floor_box.center(), vec3(1.0, 0.0, 5.0));
        assert_vec3_close(floor_box.size(), vec3(2.0, 4.0, 0.2));

        let GltfColliderShape::Sphere(orb) = z_up.colliders[1].shape else {
            panic!("sphere");
        };
        let GltfColliderShape::Sphere(old_orb) = scene.colliders[1].shape else {
            panic!("sphere");
        };
        assert_vec3_close(orb.center, turn * old_orb.center);
        assert!((orb.radius - old_orb.radius).abs() < 1e-5);
        assert_vec3_close(orb.center, z_up.nodes[3].world.transform_point3(Vec3::ZERO));

        let GltfColliderShape::Obb(ramp) = z_up.colliders[2].shape else {
            panic!("yawed box stays an Obb");
        };
        let GltfColliderShape::Obb(old_ramp) = scene.colliders[2].shape else {
            panic!("yawed box");
        };
        assert_vec3_close(ramp.center, turn * old_ramp.center);
        assert_vec3_close(ramp.half_extents, old_ramp.half_extents);
        assert!(ramp.rotation.dot(turn * old_ramp.rotation).abs() > 1.0 - 1e-4);

        assert!(z_up.colliders[3].trigger);
        let GltfColliderShape::Capsule(pill) = z_up.colliders[3].shape else {
            panic!("capsule");
        };
        let GltfColliderShape::Capsule(old_pill) = scene.colliders[3].shape else {
            panic!("capsule");
        };
        assert_vec3_close(pill.a, turn * old_pill.a);
        assert_vec3_close(pill.b, turn * old_pill.b);
        assert!((pill.radius - old_pill.radius).abs() < 1e-5);
        assert_vec3_close(z_up.spawns[0].position, turn * scene.spawns[0].position);

        let diagonal = scene.with_up_axis(vec3(0.0, 1.0, 1.0));
        assert!(
            matches!(diagonal.colliders[0].shape, GltfColliderShape::Obb(_)),
            "a diagonal up turns an axis-aligned box into an Obb"
        );
    }
}
