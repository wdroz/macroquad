//! Textured GLB conversion for [`crate::gltf::GltfScene`].
//!
//! Call [`GltfModel::from_bytes`] inside a running macroquad application: it
//! uploads embedded PNG, JPEG and WebP base-color images. Geometry, transforms,
//! normals and indices come from the fork's CPU scene loader. This module keeps
//! material metadata so applications can use their own lighting shader. It does
//! not implement PBR, skins, animation, external image URIs, or morph targets.
//! `EXT_texture_webp` is resolved in memory; the input file is never changed.
//! [`GltfModel::with_up_axis`] rotates the baked mesh with the same rule as
//! [`crate::gltf::GltfScene::with_up_axis`].

use std::{borrow::Cow, collections::HashMap};

use glam::{Mat4, Quat, Vec2, Vec3};
use miniquad::{MipmapFilterMode, TextureWrap};

use crate::{
    color::Color,
    gltf::{GltfPrimitive, GltfScene},
    models::{draw_mesh, Mesh, Vertex},
    texture::{FilterMode, Texture2D},
};

/// CPU-side material values, retained for a custom shader.
///
/// [`GltfModel::draw`] does not apply these, except that the base-color
/// factor is already written into each vertex color at 8-bit precision.
/// The exact factor stays in [`GltfSurface::base_color`].
///
/// # Example
///
/// ```
/// use macroquad::gltf_draw::{GltfAlphaMode, GltfSurface};
///
/// let surface = GltfSurface {
///     name: Some("blue".into()),
///     base_color: [0.2, 0.4, 0.6, 1.0],
///     emissive: [0.0, 0.0, 0.0],
///     metallic: 0.0,
///     roughness: 1.0,
///     transmission: 0.0,
///     alpha_mode: GltfAlphaMode::Opaque,
///     alpha_cutoff: 0.5,
///     double_sided: false,
///     unlit: false,
/// };
/// assert_eq!(surface.base_color[3], 1.0);
/// ```
#[derive(Clone, Debug)]
pub struct GltfSurface {
    /// Material name, or [`None`] when the material has none.
    pub name: Option<String>,
    /// Metallic-roughness base color factor, linear RGBA.
    pub base_color: [f32; 4],
    /// Emissive factor, linear RGB, multiplied by `KHR_materials_emissive_strength`
    /// when that extension is present. The strength defaults to 1.
    pub emissive: [f32; 3],
    /// Metallic factor, 0 to 1.
    pub metallic: f32,
    /// Roughness factor, 0 to 1.
    pub roughness: f32,
    /// `KHR_materials_transmission` factor, or 0 when the extension is absent.
    /// No refraction is applied.
    pub transmission: f32,
    /// Opaque, mask, or blend. [`GltfModel::draw`] does not sort or discard.
    pub alpha_mode: GltfAlphaMode,
    /// Mask cutoff. glTF's default of 0.5 is used when the material omits it.
    pub alpha_cutoff: f32,
    /// True when the material is double sided.
    pub double_sided: bool,
    /// True when `KHR_materials_unlit` is present.
    pub unlit: bool,
}

/// How a material's alpha is meant to be used.
///
/// [`GltfModel::draw`] ignores this. Read [`GltfSurface::alpha_mode`] and
/// draw mask or blend parts yourself.
///
/// # Example
///
/// ```
/// use macroquad::gltf_draw::GltfAlphaMode;
///
/// let mode = GltfAlphaMode::Blend;
/// assert!(matches!(mode, GltfAlphaMode::Blend));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GltfAlphaMode {
    /// Alpha is ignored. This is the glTF default.
    Opaque,
    /// Discard fragments below [`GltfSurface::alpha_cutoff`].
    Mask,
    /// Composite with the framebuffer. Draw these after opaque parts.
    Blend,
}

/// One triangle-safe, 16-bit mesh chunk, already in scene world coordinates.
///
/// # Example
///
/// ```
/// use macroquad::color::colors::WHITE;
/// use macroquad::gltf_draw::{GltfAlphaMode, GltfDrawMesh, GltfSurface};
/// use macroquad::math::{Vec3, vec3};
/// use macroquad::models::{Mesh, Vertex};
///
/// let part = GltfDrawMesh {
///     mesh: Mesh {
///         vertices: vec![Vertex::new(0.0, 0.0, 0.0, 0.0, 0.0, WHITE)],
///         indices: vec![0],
///         texture: None,
///     },
///     surface: GltfSurface {
///         name: None,
///         base_color: [1.0, 1.0, 1.0, 1.0],
///         emissive: [0.0, 0.0, 0.0],
///         metallic: 0.0,
///         roughness: 1.0,
///         transmission: 0.0,
///         alpha_mode: GltfAlphaMode::Opaque,
///         alpha_cutoff: 0.5,
///         double_sided: false,
///         unlit: true,
///     },
///     node: 0,
///     name: "box".into(),
///     path: "root/box".into(),
///     center: Vec3::ZERO,
///     bounds_min: Vec3::ZERO,
///     bounds_max: vec3(1.0, 1.0, 0.0),
/// };
/// assert_eq!(part.path, "root/box");
/// ```
#[derive(Clone)]
pub struct GltfDrawMesh {
    /// Positions, normals, and uvs after the node world matrix. Vertex colors
    /// hold the base-color factor at byte precision. `texture` is the
    /// base-color image, or [`None`].
    pub mesh: Mesh,
    /// Material values for this chunk. See [`GltfSurface`].
    pub surface: GltfSurface,
    /// Index into the CPU scene's node list.
    pub node: usize,
    /// Node name, falling back to the mesh name, then `"unnamed"`.
    pub name: String,
    /// Node and ancestor names joined by `/`.
    pub path: String,
    /// Center of this chunk's positions.
    pub center: Vec3,
    /// Minimum corner of this chunk's positions.
    pub bounds_min: Vec3,
    /// Maximum corner of this chunk's positions.
    pub bounds_max: Vec3,
}

/// Maximum vertices and indices in one [`GltfDrawMesh`].
///
/// Vertices must be in `3..=65535` because indices are `u16`. Indices must
/// be at least 3. The limit should also fit
/// [`crate::conf::Conf::draw_call_vertex_capacity`] and
/// [`crate::conf::Conf::draw_call_index_capacity`].
///
/// # Example
///
/// ```
/// use macroquad::gltf_draw::GltfMeshLimits;
///
/// let limits = GltfMeshLimits {
///     vertices: 3,
///     indices: 3,
/// };
/// assert!(limits.vertices <= GltfMeshLimits::default().vertices);
/// ```
#[derive(Clone, Copy, Debug)]
pub struct GltfMeshLimits {
    /// Vertices kept in one chunk.
    pub vertices: usize,
    /// Indices kept in one chunk.
    pub indices: usize,
}

impl Default for GltfMeshLimits {
    /// 10_000 vertices and 5_000 indices, matching [`crate::conf::Conf`]'s
    /// default draw-call capacities.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::gltf_draw::GltfMeshLimits;
    ///
    /// let limits = GltfMeshLimits::default();
    /// assert_eq!(limits.vertices, 10_000);
    /// assert_eq!(limits.indices, 5_000);
    /// ```
    fn default() -> Self {
        // Fit macroquad's default buffers. Larger applications can explicitly
        // raise both these limits and Conf's draw_call_*_capacity settings.
        Self {
            vertices: 10_000,
            indices: 5_000,
        }
    }
}

/// Counts from a [`GltfModel`] load.
///
/// # Example
///
/// ```
/// use macroquad::gltf_draw::GltfModelStats;
///
/// let stats = GltfModelStats {
///     nodes: 2,
///     meshes: 1,
///     draw_calls: 1,
///     triangles: 2,
///     textures: 1,
/// };
/// assert_eq!(stats.triangles, 2);
/// assert_eq!(GltfModelStats::default().textures, 0);
/// ```
#[derive(Clone, Copy, Debug, Default)]
pub struct GltfModelStats {
    /// Nodes in the loaded scene, including empties.
    pub nodes: usize,
    /// Meshes referenced by those nodes.
    pub meshes: usize,
    /// Number of [`GltfDrawMesh`] chunks. Equal to `parts.len()`.
    pub draw_calls: usize,
    /// Triangles across every chunk.
    pub triangles: usize,
    /// Embedded base-color images uploaded to the GPU.
    pub textures: usize,
}

/// Textured chunks for one `.glb`, plus the combined bounds.
///
/// Build one with [`GltfModel::from_bytes`] inside a running macroquad
/// application. [`GltfModel::draw`] submits every part. Read [`GltfModel::parts`]
/// for alpha and lighting values that draw ignores.
/// [`GltfModel::with_up_axis`] rotates the baked vertices so they share an up
/// axis with [`GltfScene::with_up_axis`](crate::gltf::GltfScene::with_up_axis).
///
/// # Example
///
/// ```
/// use macroquad::gltf_draw::{GltfModel, GltfModelStats};
/// use macroquad::math::Vec3;
///
/// let model = GltfModel {
///     parts: Vec::new(),
///     stats: GltfModelStats::default(),
///     bounds_min: Vec3::ZERO,
///     bounds_max: Vec3::ZERO,
///     root_names: vec!["room".into()],
/// };
/// assert_eq!(model.root_names, vec!["room".to_string()]);
/// ```
#[derive(Clone)]
pub struct GltfModel {
    /// One 16-bit chunk per entry, in node preorder.
    pub parts: Vec<GltfDrawMesh>,
    /// Counts from the load.
    pub stats: GltfModelStats,
    /// Minimum corner of every part, or zero when there are no parts.
    pub bounds_min: Vec3,
    /// Maximum corner of every part, or zero when there are no parts.
    pub bounds_max: Vec3,
    /// Name of each root node. The fallback matches [`GltfDrawMesh::name`].
    pub root_names: Vec<String>,
}

impl GltfModel {
    /// Load a binary `.glb` and upload its embedded images.
    ///
    /// Call this from a running macroquad application. `.gltf` JSON and
    /// external image URIs are errors. Chunks use [`GltfMeshLimits::default`].
    ///
    /// # Example
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    ///
    /// #[macroquad::main("Textured glTF")]
    /// async fn main() {
    ///     let model = GltfModel::from_bytes(include_bytes!(
    ///         "../examples/assets/textured_quad.glb"
    ///     ))
    ///     .expect("checkerboard");
    ///     assert!(model.stats.textures > 0);
    /// }
    /// ```
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        Self::from_bytes_with_limits(bytes, GltfMeshLimits::default())
    }

    /// [`GltfModel::from_bytes`] with an explicit chunk size.
    ///
    /// `limits.vertices` must be in `3..=65535` and `limits.indices` must be
    /// at least 3. Raise [`crate::conf::Conf`]'s draw-call capacities when
    /// the limit is larger than the default.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    ///
    /// #[macroquad::main("Textured glTF")]
    /// async fn main() {
    ///     let limits = GltfMeshLimits {
    ///         vertices: 10_000,
    ///         indices: 5_000,
    ///     };
    ///     let model = GltfModel::from_bytes_with_limits(
    ///         include_bytes!("../examples/assets/textured_quad.glb"),
    ///         limits,
    ///     )
    ///     .expect("checkerboard");
    ///     assert!(!model.parts.is_empty());
    /// }
    /// ```
    pub fn from_bytes_with_limits(bytes: &[u8], limits: GltfMeshLimits) -> Result<Self, String> {
        validate_limits(limits)?;
        let (scene, document, binary) = prepare_glb(bytes)?;
        // The CPU loader visits nodes in preorder and meshes in first-use order.
        // Recreate that same source-mesh mapping, rather than assuming indices
        // happen to match the file (unused meshes and instances are legal).
        let mut source_meshes = Vec::new();
        let mut seen = std::collections::HashSet::new();
        fn visit(
            node: gltf::Node<'_>,
            seen: &mut std::collections::HashSet<usize>,
            out: &mut Vec<usize>,
        ) {
            if let Some(mesh) = node.mesh() {
                if seen.insert(mesh.index()) {
                    out.push(mesh.index());
                }
            }
            for child in node.children() {
                visit(child, seen, out);
            }
        }
        if let Some(source_scene) = document
            .default_scene()
            .or_else(|| document.scenes().next())
        {
            for node in source_scene.nodes() {
                visit(node, &mut seen, &mut source_meshes);
            }
        }
        let source_meshes: Vec<_> = source_meshes
            .iter()
            .map(|index| document.meshes().nth(*index).unwrap())
            .collect();
        let mut textures: HashMap<usize, Texture2D> = HashMap::new();
        let mut parts = Vec::new();
        let mut bounds_min = Vec3::splat(f32::INFINITY);
        let mut bounds_max = Vec3::splat(f32::NEG_INFINITY);
        let mut root_names = Vec::new();
        let mut paths: Vec<String> = Vec::with_capacity(scene.nodes.len());
        for (node_index, node) in scene.nodes.iter().enumerate() {
            let mesh_name = node
                .mesh
                .and_then(|index| scene.meshes[index].name.as_deref());
            let name = node
                .name
                .as_deref()
                .or(mesh_name)
                .unwrap_or("unnamed")
                .to_string();
            let path = if let Some(parent) = node.parent {
                format!("{}/{}", paths[parent], name)
            } else {
                name.clone()
            };
            paths.push(path.clone());
            if node.parent.is_none() {
                root_names.push(name.clone());
            }
            let Some(mesh_index) = node.mesh else {
                continue;
            };
            let source_mesh = &source_meshes[mesh_index];
            for (primitive, source) in scene.meshes[mesh_index]
                .primitives
                .iter()
                .zip(source_mesh.primitives())
            {
                let material = source.material();
                let pbr = material.pbr_metallic_roughness();
                let texture = if let Some(info) = pbr.base_color_texture() {
                    let texture = info.texture();
                    let key = texture.index();
                    if !textures.contains_key(&key) {
                        let image = texture.source();
                        let data = match image.source() {
                            gltf::image::Source::View { view, .. } => {
                                if view.buffer().index() != 0
                                    || !matches!(view.buffer().source(), gltf::buffer::Source::Bin)
                                {
                                    return Err(
                                        "image buffers must use the GLB binary chunk".into()
                                    );
                                }
                                let end = view
                                    .offset()
                                    .checked_add(view.length())
                                    .ok_or("image buffer range overflow")?;
                                binary
                                    .get(view.offset()..end)
                                    .ok_or("image buffer is out of bounds")?
                            }
                            gltf::image::Source::Uri { .. } => {
                                return Err(
                                    "external image URIs are unsupported; embed images in the GLB"
                                        .into(),
                                )
                            }
                        };
                        let rgba = image::load_from_memory(data)
                            .map_err(|e| format!("image {}: {e}", image.index()))?
                            .to_rgba8();
                        let (mut min_filter, mut mipmap_filter) =
                            minification_filters(texture.sampler().min_filter());
                        let version = crate::get_quad_context().info().gl_version_string;
                        let restricted_npot = (!rgba.width().is_power_of_two()
                            || !rgba.height().is_power_of_two())
                            && restricted_npot_backend(&version);
                        // Preserve original pixels. Legacy WebGL 1 / GLES 2
                        // cannot repeat or mipmap non-power-of-two textures.
                        if restricted_npot {
                            min_filter = FilterMode::Linear;
                            mipmap_filter = MipmapFilterMode::None;
                        }
                        let has_mipmaps = mipmap_filter != MipmapFilterMode::None;
                        let width = u16::try_from(rgba.width())
                            .map_err(|_| "texture width exceeds 65535")?;
                        let height = u16::try_from(rgba.height())
                            .map_err(|_| "texture height exceeds 65535")?;
                        let gpu = if has_mipmaps {
                            Texture2D::from_rgba8_mipmapped(width, height, rgba.as_raw())
                        } else {
                            Texture2D::from_rgba8(width, height, rgba.as_raw())
                        };
                        let nearest = matches!(
                            texture.sampler().mag_filter(),
                            Some(gltf::texture::MagFilter::Nearest)
                        );
                        let mag_filter = if nearest {
                            FilterMode::Nearest
                        } else {
                            FilterMode::Linear
                        };
                        fn wrap(mode: gltf::texture::WrappingMode) -> TextureWrap {
                            match mode {
                                gltf::texture::WrappingMode::ClampToEdge => TextureWrap::Clamp,
                                gltf::texture::WrappingMode::MirroredRepeat => TextureWrap::Mirror,
                                gltf::texture::WrappingMode::Repeat => TextureWrap::Repeat,
                            }
                        }
                        unsafe {
                            let gl = crate::window::get_internal_gl();
                            gl.quad_context.texture_set_min_filter(
                                gpu.raw_miniquad_id(),
                                min_filter,
                                mipmap_filter,
                            );
                            gl.quad_context
                                .texture_set_mag_filter(gpu.raw_miniquad_id(), mag_filter);
                            gl.quad_context.texture_set_wrap(
                                gpu.raw_miniquad_id(),
                                if restricted_npot {
                                    TextureWrap::Clamp
                                } else {
                                    wrap(texture.sampler().wrap_s())
                                },
                                if restricted_npot {
                                    TextureWrap::Clamp
                                } else {
                                    wrap(texture.sampler().wrap_t())
                                },
                            );
                        }
                        textures.insert(key, gpu);
                    }
                    textures.get(&key).cloned()
                } else {
                    None
                };
                let strength = material
                    .extension_value("KHR_materials_emissive_strength")
                    .and_then(|v| v.get("emissiveStrength"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(1.0) as f32;
                let mut emissive = material.emissive_factor();
                for channel in &mut emissive {
                    *channel *= strength;
                }
                let surface = GltfSurface {
                    name: material.name().map(str::to_string),
                    base_color: pbr.base_color_factor(),
                    emissive,
                    metallic: pbr.metallic_factor(),
                    roughness: pbr.roughness_factor(),
                    transmission: material
                        .extension_value("KHR_materials_transmission")
                        .and_then(|v| v.get("transmissionFactor"))
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0) as f32,
                    alpha_mode: match material.alpha_mode() {
                        gltf::material::AlphaMode::Opaque => GltfAlphaMode::Opaque,
                        gltf::material::AlphaMode::Mask => GltfAlphaMode::Mask,
                        gltf::material::AlphaMode::Blend => GltfAlphaMode::Blend,
                    },
                    alpha_cutoff: material.alpha_cutoff().unwrap_or(0.5),
                    double_sided: material.double_sided(),
                    unlit: material.extension_value("KHR_materials_unlit").is_some(),
                };
                let mut vertices = world_vertices(primitive, node.world);
                if let Some(info) = pbr.base_color_texture() {
                    apply_material_uvs(&mut vertices, &source, binary.as_ref(), &info)?;
                }
                let indices = winding_indices(&primitive.indices, node.world);
                for (vertices, indices) in split_triangles(&vertices, &indices, limits)? {
                    let mut min = Vec3::splat(f32::INFINITY);
                    let mut max = Vec3::splat(f32::NEG_INFINITY);
                    for vertex in &vertices {
                        min = min.min(vertex.position);
                        max = max.max(vertex.position);
                    }
                    bounds_min = bounds_min.min(min);
                    bounds_max = bounds_max.max(max);
                    parts.push(GltfDrawMesh {
                        mesh: Mesh {
                            vertices,
                            indices,
                            texture: texture.clone(),
                        },
                        surface: surface.clone(),
                        node: node_index,
                        name: name.clone(),
                        path: path.clone(),
                        center: (min + max) * 0.5,
                        bounds_min: min,
                        bounds_max: max,
                    });
                }
            }
        }
        if parts.is_empty() {
            bounds_min = Vec3::ZERO;
            bounds_max = Vec3::ZERO;
        }
        let stats = GltfModelStats {
            nodes: scene.nodes.len(),
            meshes: scene.meshes.len(),
            draw_calls: parts.len(),
            triangles: parts.iter().map(|p| p.mesh.indices.len() / 3).sum(),
            textures: textures.len(),
        };
        Ok(Self {
            parts,
            stats,
            bounds_min,
            bounds_max,
            root_names,
        })
    }

    /// Rotates baked vertices so glTF +Y points along `up`.
    ///
    /// [`Self::from_bytes`] keeps the file's Y-up axes and bakes each node's
    /// world matrix into positions and normals. This rotates those world
    /// positions and normals, then rebuilds each chunk's bounds from the
    /// rotated positions. UVs, vertex colors, indices, materials, names, and
    /// textures stay. A rotation does not flip winding; a mirrored node was
    /// already corrected at load time.
    ///
    /// [`Vec3::Y`], any positive multiple of it, a zero vector, and a
    /// non-finite vector return a copy with the same vertices. `up` is a
    /// direction: its length is ignored.
    ///
    /// Pass the same `up` to [`GltfScene::with_up_axis`](crate::gltf::GltfScene::with_up_axis) so colliders,
    /// triggers, and spawns match the mesh. The two types parse the file
    /// separately, and calling one does not change the other.
    ///
    /// [`Camera3D`](crate::camera::Camera3D)'s [`Default`] up axis is +Z, not
    /// +Y. This method does not change that default. A camera aimed at a
    /// model from [`Self::from_bytes`] needs `up: Vec3::Y`, or the model and
    /// the scene can be turned with `with_up_axis(Vec3::Z)`.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::camera::Camera3D;
    /// use macroquad::color::colors::WHITE;
    /// use macroquad::gltf_draw::{
    ///     GltfAlphaMode, GltfDrawMesh, GltfModel, GltfModelStats, GltfSurface,
    /// };
    /// use macroquad::math::{vec3, Vec3};
    /// use macroquad::models::{Mesh, Vertex};
    ///
    /// let model = GltfModel {
    ///     parts: vec![GltfDrawMesh {
    ///         mesh: Mesh {
    ///             vertices: vec![Vertex::new(0.0, 2.0, 0.0, 0.0, 0.0, WHITE)],
    ///             indices: vec![0],
    ///             texture: None,
    ///         },
    ///         surface: GltfSurface {
    ///             name: None,
    ///             base_color: [1.0, 1.0, 1.0, 1.0],
    ///             emissive: [0.0, 0.0, 0.0],
    ///             metallic: 0.0,
    ///             roughness: 1.0,
    ///             transmission: 0.0,
    ///             alpha_mode: GltfAlphaMode::Opaque,
    ///             alpha_cutoff: 0.5,
    ///             double_sided: false,
    ///             unlit: true,
    ///         },
    ///         node: 0,
    ///         name: "post".into(),
    ///         path: "post".into(),
    ///         center: vec3(0.0, 2.0, 0.0),
    ///         bounds_min: vec3(0.0, 2.0, 0.0),
    ///         bounds_max: vec3(0.0, 2.0, 0.0),
    ///     }],
    ///     stats: GltfModelStats::default(),
    ///     bounds_min: vec3(0.0, 2.0, 0.0),
    ///     bounds_max: vec3(0.0, 2.0, 0.0),
    ///     root_names: vec!["post".into()],
    /// };
    /// assert_eq!(
    ///     model.with_up_axis(Vec3::Y).parts[0].mesh.vertices[0].position,
    ///     vec3(0.0, 2.0, 0.0)
    /// );
    /// let z_up = model.with_up_axis(Vec3::Z);
    /// assert!(
    ///     (z_up.parts[0].mesh.vertices[0].position - vec3(0.0, 0.0, 2.0)).length() < 1.0e-4
    /// );
    /// assert_eq!(Camera3D::default().up, Vec3::Z);
    /// ```
    pub fn with_up_axis(&self, up: Vec3) -> Self {
        let Some(rotation) = crate::gltf::up_axis_rotation(up) else {
            return self.clone();
        };
        let mut model = self.clone();
        for part in &mut model.parts {
            if part.mesh.vertices.is_empty() {
                let (min, max) = rotated_bounds(rotation, part.bounds_min, part.bounds_max);
                part.bounds_min = min;
                part.bounds_max = max;
                part.center = (min + max) * 0.5;
            } else {
                let mut min = Vec3::splat(f32::INFINITY);
                let mut max = Vec3::splat(f32::NEG_INFINITY);
                for vertex in &mut part.mesh.vertices {
                    vertex.position = rotation * vertex.position;
                    let normal = rotation * vertex.normal.truncate();
                    vertex.normal = normal.extend(vertex.normal.w);
                    min = min.min(vertex.position);
                    max = max.max(vertex.position);
                }
                part.bounds_min = min;
                part.bounds_max = max;
                part.center = (min + max) * 0.5;
            }
        }
        if model.parts.is_empty() {
            let (min, max) = rotated_bounds(rotation, model.bounds_min, model.bounds_max);
            model.bounds_min = min;
            model.bounds_max = max;
        } else {
            let mut min = Vec3::splat(f32::INFINITY);
            let mut max = Vec3::splat(f32::NEG_INFINITY);
            for part in &model.parts {
                min = min.min(part.bounds_min);
                max = max.max(part.bounds_max);
            }
            model.bounds_min = min;
            model.bounds_max = max;
        }
        model
    }

    /// Submit textured meshes with their base color, using the current material
    /// and camera. Applications implementing alpha or lighting should inspect
    /// `parts`. The base-color factor is already in each vertex's color field.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    ///
    /// #[macroquad::main("Textured glTF")]
    /// async fn main() {
    ///     let model = GltfModel::from_bytes(include_bytes!(
    ///         "../examples/assets/textured_quad.glb"
    ///     ))
    ///     .expect("checkerboard");
    ///     model.draw();
    /// }
    /// ```
    pub fn draw(&self) {
        for part in &self.parts {
            draw_mesh(&part.mesh);
        }
    }
}

/// Axis-aligned bounds of `min`/`max` after `rotation`.
///
/// Non-finite corners are left in place. A pure rotation of the eight corners
/// is the new box when the chunk has no vertices to measure.
fn rotated_bounds(rotation: Quat, min: Vec3, max: Vec3) -> (Vec3, Vec3) {
    if !min.is_finite() || !max.is_finite() {
        return (min, max);
    }
    let mut new_min = Vec3::splat(f32::INFINITY);
    let mut new_max = Vec3::splat(f32::NEG_INFINITY);
    for x in [min.x, max.x] {
        for y in [min.y, max.y] {
            for z in [min.z, max.z] {
                let point = rotation * Vec3::new(x, y, z);
                new_min = new_min.min(point);
                new_max = new_max.max(point);
            }
        }
    }
    (new_min, new_max)
}

fn minification_filters(
    filter: Option<gltf::texture::MinFilter>,
) -> (FilterMode, MipmapFilterMode) {
    use gltf::texture::MinFilter as F;
    match filter {
        Some(F::Nearest) => (FilterMode::Nearest, MipmapFilterMode::None),
        Some(F::Linear) => (FilterMode::Linear, MipmapFilterMode::None),
        Some(F::NearestMipmapNearest) => (FilterMode::Nearest, MipmapFilterMode::Nearest),
        Some(F::LinearMipmapNearest) => (FilterMode::Linear, MipmapFilterMode::Nearest),
        Some(F::NearestMipmapLinear) => (FilterMode::Nearest, MipmapFilterMode::Linear),
        Some(F::LinearMipmapLinear) | None => (FilterMode::Linear, MipmapFilterMode::Linear),
    }
}

fn restricted_npot_backend(version: &str) -> bool {
    version.contains("WebGL 1.") || version.contains("OpenGL ES 2.")
}

fn prepare_glb(bytes: &[u8]) -> Result<(GltfScene, gltf::Document, Cow<'_, [u8]>), String> {
    let glb = gltf::binary::Glb::from_slice(bytes).map_err(|e| e.to_string())?;
    let binary = glb.bin.ok_or("GLB has no binary chunk")?;
    let mut json: serde_json::Value =
        serde_json::from_slice(&glb.json).map_err(|e| e.to_string())?;
    normalize_webp(&mut json)?;
    let json_bytes = serde_json::to_vec(&json).map_err(|e| e.to_string())?;
    // Validate normally after resolving the extensions implemented by this
    // adapter. Unrecognized required extensions are retained and still fail.
    let document = gltf::Gltf::from_slice(&json_bytes)
        .map_err(|e| e.to_string())?
        .document;
    let normalized = gltf::binary::Glb {
        header: glb.header,
        json: Cow::Owned(json_bytes),
        bin: Some(Cow::Borrowed(binary.as_ref())),
    }
    .to_vec()
    .map_err(|e| e.to_string())?;
    let scene = GltfScene::from_bytes(&normalized).map_err(|e| e.to_string())?;
    Ok((scene, document, binary))
}

fn normalize_webp(json: &mut serde_json::Value) -> Result<(), String> {
    let image_count = json
        .get("images")
        .and_then(|v| v.as_array())
        .map_or(0, Vec::len);
    if let Some(textures) = json.get_mut("textures").and_then(|v| v.as_array_mut()) {
        for texture in textures {
            if let Some(source) = texture
                .pointer("/extensions/EXT_texture_webp/source")
                .cloned()
            {
                let index = source
                    .as_u64()
                    .ok_or("EXT_texture_webp source is not an unsigned integer")?;
                if index >= image_count as u64 {
                    return Err("EXT_texture_webp source is out of bounds".into());
                }
                texture["source"] = source;
            }
        }
    }
    if let Some(required) = json
        .get_mut("extensionsRequired")
        .and_then(|v| v.as_array_mut())
    {
        required.retain(|name| {
            !matches!(
                name.as_str(),
                Some(
                    "EXT_texture_webp"
                        | "KHR_materials_unlit"
                        | "KHR_materials_emissive_strength"
                        | "KHR_texture_transform"
                )
            )
        });
    }
    Ok(())
}

fn world_vertices(primitive: &GltfPrimitive, world: Mat4) -> Vec<Vertex> {
    let normals = if primitive.normals.is_empty() {
        let mut normals = vec![Vec3::ZERO; primitive.positions.len()];
        for triangle in primitive.indices.chunks_exact(3) {
            let [a, b, c] = [
                triangle[0] as usize,
                triangle[1] as usize,
                triangle[2] as usize,
            ];
            let normal = (primitive.positions[b] - primitive.positions[a])
                .cross(primitive.positions[c] - primitive.positions[a]);
            normals[a] += normal;
            normals[b] += normal;
            normals[c] += normal;
        }
        normals
    } else {
        primitive.normals.clone()
    };
    let normal_matrix = world.inverse().transpose();
    primitive
        .positions
        .iter()
        .enumerate()
        .map(|(i, position)| {
            let [r, g, b, a] = primitive.base_color_factor;
            let mut vertex = Vertex::new2(
                world.transform_point3(*position),
                primitive.uvs.get(i).copied().unwrap_or(Vec2::ZERO),
                Color::new(
                    r.clamp(0.0, 1.0),
                    g.clamp(0.0, 1.0),
                    b.clamp(0.0, 1.0),
                    a.clamp(0.0, 1.0),
                ),
            );
            vertex.normal = normal_matrix
                .transform_vector3(normals[i])
                .normalize_or_zero()
                .extend(0.0);
            vertex
        })
        .collect()
}

fn apply_material_uvs(
    vertices: &mut [Vertex],
    primitive: &gltf::Primitive<'_>,
    binary: &[u8],
    info: &gltf::texture::Info<'_>,
) -> Result<(), String> {
    let transform = info.extension_value("KHR_texture_transform");
    let tex_coord = transform
        .and_then(|t| t.get("texCoord"))
        .and_then(|v| v.as_u64())
        .map_or(info.tex_coord(), |v| v as u32);
    let reader = primitive.reader(|buffer| {
        if buffer.index() == 0 {
            Some(binary)
        } else {
            None
        }
    });
    let uvs = reader.read_tex_coords(tex_coord).ok_or_else(|| {
        format!(
            "material {:?} requires missing TEXCOORD_{tex_coord}",
            primitive.material().name()
        )
    })?;
    let uvs: Vec<_> = uvs.into_f32().collect();
    if uvs.len() != vertices.len() {
        return Err("texture-coordinate count differs from position count".into());
    }
    let pair = |name: &str, default: Vec2| -> Vec2 {
        transform
            .and_then(|t| t.get(name))
            .and_then(|v| v.as_array())
            .filter(|v| v.len() == 2)
            .and_then(|v| Some(Vec2::new(v[0].as_f64()? as f32, v[1].as_f64()? as f32)))
            .unwrap_or(default)
    };
    let offset = pair("offset", Vec2::ZERO);
    let scale = pair("scale", Vec2::ONE);
    let rotation = transform
        .and_then(|t| t.get("rotation"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32;
    let (sin, cos) = rotation.sin_cos();
    for (vertex, uv) in vertices.iter_mut().zip(uvs) {
        let uv = Vec2::from_array(uv) * scale;
        vertex.uv = offset + Vec2::new(cos * uv.x - sin * uv.y, sin * uv.x + cos * uv.y);
    }
    Ok(())
}

fn winding_indices(indices: &[u32], world: Mat4) -> Cow<'_, [u32]> {
    if world.determinant() < 0.0 {
        let mut reversed = indices.to_vec();
        for triangle in reversed.chunks_exact_mut(3) {
            triangle.swap(1, 2);
        }
        Cow::Owned(reversed)
    } else {
        Cow::Borrowed(indices)
    }
}

fn validate_limits(limits: GltfMeshLimits) -> Result<(), String> {
    if limits.vertices < 3 || limits.vertices > u16::MAX as usize || limits.indices < 3 {
        return Err("mesh limits need 3..=65535 vertices and at least 3 indices".into());
    }
    Ok(())
}

fn split_triangles(
    vertices: &[Vertex],
    indices: &[u32],
    limits: GltfMeshLimits,
) -> Result<Vec<(Vec<Vertex>, Vec<u16>)>, String> {
    validate_limits(limits)?;
    if indices.len() % 3 != 0 {
        return Err("triangle index count is not a multiple of three".into());
    }
    let mut result = Vec::new();
    let mut chunk_vertices = Vec::new();
    let mut chunk_indices = Vec::new();
    let mut remap: HashMap<u32, u16> = HashMap::new();
    for triangle in indices.chunks_exact(3) {
        if triangle
            .iter()
            .any(|&index| index as usize >= vertices.len())
        {
            return Err("triangle index is out of bounds".into());
        }
        let additional = triangle
            .iter()
            .filter(|index| !remap.contains_key(index))
            .count();
        if !chunk_indices.is_empty()
            && (chunk_vertices.len() + additional > limits.vertices
                || chunk_indices.len() + 3 > limits.indices)
        {
            result.push((
                std::mem::take(&mut chunk_vertices),
                std::mem::take(&mut chunk_indices),
            ));
            remap.clear();
        }
        for &index in triangle {
            let mapped = *remap.entry(index).or_insert_with(|| {
                let mapped = chunk_vertices.len() as u16;
                chunk_vertices.push(vertices[index as usize]);
                mapped
            });
            chunk_indices.push(mapped);
        }
    }
    if !chunk_indices.is_empty() {
        result.push((chunk_vertices, chunk_indices));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::WHITE;

    #[test]
    fn gltf_minification_keeps_texel_and_mipmap_filters_independent() {
        use gltf::texture::MinFilter as F;
        assert_eq!(
            minification_filters(Some(F::Nearest)),
            (FilterMode::Nearest, MipmapFilterMode::None)
        );
        assert_eq!(
            minification_filters(Some(F::Linear)),
            (FilterMode::Linear, MipmapFilterMode::None)
        );
        assert_eq!(
            minification_filters(Some(F::NearestMipmapNearest)),
            (FilterMode::Nearest, MipmapFilterMode::Nearest)
        );
        assert_eq!(
            minification_filters(Some(F::LinearMipmapNearest)),
            (FilterMode::Linear, MipmapFilterMode::Nearest)
        );
        assert_eq!(
            minification_filters(Some(F::NearestMipmapLinear)),
            (FilterMode::Nearest, MipmapFilterMode::Linear)
        );
        assert_eq!(
            minification_filters(None),
            (FilterMode::Linear, MipmapFilterMode::Linear)
        );
    }

    #[test]
    fn npot_fallback_is_restricted_to_legacy_embedded_backends() {
        assert!(restricted_npot_backend(
            "WebGL 1.0 (OpenGL ES 2.0 Chromium)"
        ));
        assert!(restricted_npot_backend("OpenGL ES 2.0 Mesa"));
        assert!(!restricted_npot_backend(
            "WebGL 2.0 (OpenGL ES 3.0 Chromium)"
        ));
        assert!(!restricted_npot_backend("4.6.0 NVIDIA"));
        assert!(!restricted_npot_backend("")); // Metal.
    }

    fn fixture_with_json(edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
        let fixture =
            gltf::binary::Glb::from_slice(include_bytes!("../examples/assets/textured_quad.glb"))
                .unwrap();
        let mut json: serde_json::Value = serde_json::from_slice(&fixture.json).unwrap();
        edit(&mut json);
        gltf::binary::Glb {
            header: fixture.header,
            json: Cow::Owned(serde_json::to_vec(&json).unwrap()),
            bin: fixture.bin,
        }
        .to_vec()
        .unwrap()
    }

    #[test]
    fn selected_uv_set_and_required_texture_transform_are_applied() {
        let bytes = fixture_with_json(|json| {
            let attributes = &mut json["meshes"][0]["primitives"][0]["attributes"];
            attributes.as_object_mut().unwrap().remove("TEXCOORD_0");
            attributes["TEXCOORD_1"] = serde_json::json!(2);
            json["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"]["extensions"] = serde_json::json!({"KHR_texture_transform":{"texCoord":1,"offset":[0.5,0.25],"scale":[2,3],"rotation":std::f64::consts::FRAC_PI_2}});
            json["extensionsUsed"] = serde_json::json!(["KHR_texture_transform"]);
            json["extensionsRequired"] = serde_json::json!(["KHR_texture_transform"]);
        });
        let (scene, document, binary) = prepare_glb(&bytes).unwrap();
        let primitive = document
            .meshes()
            .next()
            .unwrap()
            .primitives()
            .next()
            .unwrap();
        let mut vertices = world_vertices(&scene.meshes[0].primitives[0], Mat4::IDENTITY);
        let info = primitive
            .material()
            .pbr_metallic_roughness()
            .base_color_texture()
            .unwrap();
        apply_material_uvs(&mut vertices, &primitive, binary.as_ref(), &info).unwrap();
        assert!((vertices[0].uv - Vec2::new(-2.5, 0.25)).length() < 0.00001);
    }

    #[test]
    fn implemented_required_material_extensions_keep_their_metadata() {
        let bytes = fixture_with_json(|json| {
            json["materials"][0]["extensions"] = serde_json::json!({"KHR_materials_unlit":{},"KHR_materials_emissive_strength":{"emissiveStrength":4}});
            json["extensionsUsed"] =
                serde_json::json!(["KHR_materials_unlit", "KHR_materials_emissive_strength"]);
            json["extensionsRequired"] = json["extensionsUsed"].clone();
        });
        let (_, document, _) = prepare_glb(&bytes).unwrap();
        let material = document.materials().next().unwrap();
        assert!(material.extension_value("KHR_materials_unlit").is_some());
        assert_eq!(
            material
                .extension_value("KHR_materials_emissive_strength")
                .unwrap()["emissiveStrength"],
            4
        );
    }

    #[test]
    fn mirrored_node_keeps_front_face_winding_consistent_with_normals() {
        let primitive = GltfPrimitive {
            positions: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
            normals: vec![Vec3::Z; 3],
            uvs: vec![],
            indices: vec![0, 1, 2],
            base_color_factor: [1.; 4],
            base_color_texture: None,
        };
        let transform = Mat4::from_scale(Vec3::new(-2.0, 1.0, 1.0));
        let vertices = world_vertices(&primitive, transform);
        let indices = winding_indices(&primitive.indices, transform);
        assert_eq!(indices.as_ref(), &[0, 2, 1]);
        let a = vertices[indices[0] as usize].position;
        let b = vertices[indices[1] as usize].position;
        let c = vertices[indices[2] as usize].position;
        assert!((b - a).cross(c - a).dot(vertices[0].normal.truncate()) > 0.0);
    }

    #[test]
    fn embedded_png_fixture_loads_through_cpu_scene() {
        let (scene, document, binary) =
            prepare_glb(include_bytes!("../examples/assets/textured_quad.glb")).unwrap();
        assert_eq!(scene.nodes.len(), 1);
        assert_eq!(scene.meshes[0].primitives[0].indices, [0, 1, 2, 0, 2, 3]);
        let image = document.images().next().unwrap();
        let gltf::image::Source::View { view, .. } = image.source() else {
            panic!("embedded image expected");
        };
        let decoded =
            image::load_from_memory(&binary[view.offset()..view.offset() + view.length()]).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (4, 4));
    }

    #[test]
    fn required_webp_glb_decodes_without_a_standard_texture_source() {
        let fixture =
            gltf::binary::Glb::from_slice(include_bytes!("../examples/assets/textured_quad.glb"))
                .unwrap();
        let mut json: serde_json::Value = serde_json::from_slice(&fixture.json).unwrap();
        let mut webp = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut webp)
            .encode(&[17, 42, 89, 255], 1, 1, image::ColorType::Rgba8)
            .unwrap();
        let mut binary = fixture.bin.unwrap().as_ref()[..140].to_vec();
        binary.extend_from_slice(&webp);
        json["buffers"][0]["byteLength"] = serde_json::json!(binary.len());
        json["bufferViews"][4]["byteLength"] = serde_json::json!(webp.len());
        json["images"][0]["mimeType"] = serde_json::json!("image/webp");
        json["textures"][0]
            .as_object_mut()
            .unwrap()
            .remove("source");
        json["textures"][0]["extensions"] = serde_json::json!({"EXT_texture_webp":{"source":0}});
        json["extensionsUsed"] = serde_json::json!(["EXT_texture_webp"]);
        json["extensionsRequired"] = serde_json::json!(["EXT_texture_webp"]);
        let bytes = gltf::binary::Glb {
            header: fixture.header,
            json: Cow::Owned(serde_json::to_vec(&json).unwrap()),
            bin: Some(Cow::Owned(binary)),
        }
        .to_vec()
        .unwrap();
        let (_, document, binary) = prepare_glb(&bytes).unwrap();
        let texture = document.textures().next().unwrap();
        assert_eq!(texture.source().index(), 0);
        let gltf::image::Source::View { view, .. } = texture.source().source() else {
            panic!("embedded image expected");
        };
        let decoded =
            image::load_from_memory(&binary[view.offset()..view.offset() + view.length()])
                .unwrap()
                .to_rgba8();
        assert_eq!(decoded.as_raw(), &[17, 42, 89, 255]);
    }

    #[test]
    fn splitting_preserves_large_indices_and_triangle_order() {
        let vertices: Vec<_> = (0..70_002)
            .map(|i| Vertex::new2(Vec3::new(i as f32, 0., 0.), Vec2::ZERO, WHITE))
            .collect();
        let indices: Vec<_> = (0..70_002).collect();
        let chunks = split_triangles(
            &vertices,
            &indices,
            GltfMeshLimits {
                vertices: 60_000,
                indices: 60_000,
            },
        )
        .unwrap();
        assert_eq!(chunks.len(), 2);
        let restored: Vec<_> = chunks
            .iter()
            .flat_map(|(v, i)| i.iter().map(|&index| v[index as usize].position.x as u32))
            .collect();
        assert_eq!(restored, indices);
        assert!(chunks
            .iter()
            .all(|(v, i)| v.len() <= 60_000 && i.len() <= 60_000 && i.len() % 3 == 0));
    }

    #[test]
    fn splitting_reuses_vertices_and_respects_index_capacity() {
        let vertices = vec![Vertex::new2(Vec3::ZERO, Vec2::ZERO, WHITE); 3];
        let chunks = split_triangles(
            &vertices,
            &[0, 1, 2, 0, 2, 1, 0, 1, 2],
            GltfMeshLimits {
                vertices: 3,
                indices: 6,
            },
        )
        .unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].0.len(), 3);
        assert_eq!(chunks[0].1.len(), 6);
        assert!(split_triangles(&vertices, &[0, 1, 8], GltfMeshLimits::default()).is_err());
    }

    #[test]
    fn webp_source_is_resolved_without_dropping_other_requirements() {
        let mut json = serde_json::json!({"images":[{},{}],"textures":[{"source":0,"extensions":{"EXT_texture_webp":{"source":1}}}],"extensionsRequired":["EXT_texture_webp","UNKNOWN_extension"]});
        normalize_webp(&mut json).unwrap();
        assert_eq!(json["textures"][0]["source"], 1);
        assert_eq!(
            json["extensionsRequired"],
            serde_json::json!(["UNKNOWN_extension"])
        );
        json["textures"][0]["extensions"]["EXT_texture_webp"]["source"] = serde_json::json!(9);
        assert!(normalize_webp(&mut json).is_err());
    }

    #[test]
    fn normals_use_inverse_transpose_for_nonuniform_scale() {
        let primitive = GltfPrimitive {
            positions: vec![Vec3::ZERO],
            normals: vec![Vec3::new(1., 1., 0.).normalize()],
            uvs: vec![],
            indices: vec![],
            base_color_factor: [1.; 4],
            base_color_texture: None,
        };
        let vertices = world_vertices(&primitive, Mat4::from_scale(Vec3::new(2., 1., 1.)));
        let expected = Vec3::new(0.5, 1., 0.).normalize();
        assert!((vertices[0].normal.truncate() - expected).length() < 0.00001);
    }

    fn close(left: Vec3, right: Vec3) {
        assert!(
            (left - right).length() < 1e-4,
            "left {left:?} right {right:?}"
        );
    }

    fn sample_surface() -> GltfSurface {
        GltfSurface {
            name: Some("blue".into()),
            base_color: [0.2, 0.4, 0.6, 1.0],
            emissive: [0.1, 0.0, 0.0],
            metallic: 0.2,
            roughness: 0.8,
            transmission: 0.0,
            alpha_mode: GltfAlphaMode::Mask,
            alpha_cutoff: 0.4,
            double_sided: true,
            unlit: true,
        }
    }

    fn model_from_parts(parts: Vec<GltfDrawMesh>) -> GltfModel {
        let (bounds_min, bounds_max) = if parts.is_empty() {
            (Vec3::ZERO, Vec3::ZERO)
        } else {
            let mut min = Vec3::splat(f32::INFINITY);
            let mut max = Vec3::splat(f32::NEG_INFINITY);
            for part in &parts {
                min = min.min(part.bounds_min);
                max = max.max(part.bounds_max);
            }
            (min, max)
        };
        GltfModel {
            stats: GltfModelStats {
                nodes: 2,
                meshes: 1,
                draw_calls: parts.len(),
                triangles: 1,
                textures: 0,
            },
            parts,
            bounds_min,
            bounds_max,
            root_names: vec!["root".into()],
        }
    }

    fn part_with_vertices(vertices: Vec<Vertex>, indices: Vec<u16>, name: &str) -> GltfDrawMesh {
        let (min, max) = if vertices.is_empty() {
            (Vec3::ZERO, Vec3::ZERO)
        } else {
            let mut min = Vec3::splat(f32::INFINITY);
            let mut max = Vec3::splat(f32::NEG_INFINITY);
            for vertex in &vertices {
                min = min.min(vertex.position);
                max = max.max(vertex.position);
            }
            (min, max)
        };
        GltfDrawMesh {
            mesh: Mesh {
                vertices,
                indices,
                texture: None,
            },
            surface: sample_surface(),
            node: 1,
            name: name.into(),
            path: format!("root/{name}"),
            center: (min + max) * 0.5,
            bounds_min: min,
            bounds_max: max,
        }
    }

    fn vertex_at(position: Vec3, normal: glam::Vec4) -> Vertex {
        Vertex {
            position,
            uv: Vec2::new(0.25, 0.75),
            color: [9, 8, 7, 6],
            normal,
        }
    }

    #[test]
    fn y_up_and_an_unusable_up_leave_the_model_unchanged() {
        let model = model_from_parts(vec![part_with_vertices(
            vec![vertex_at(
                Vec3::new(1.0, 2.0, 3.0),
                glam::vec4(0.0, 1.0, 0.0, 0.5),
            )],
            vec![0, 2, 1],
            "post",
        )]);
        for up in [Vec3::Y, Vec3::Y * 4.0, Vec3::ZERO, Vec3::NAN] {
            let turned = model.with_up_axis(up);
            let before = &model.parts[0];
            let after = &turned.parts[0];
            assert_eq!(
                after.mesh.vertices[0].position,
                before.mesh.vertices[0].position
            );
            assert_eq!(
                after.mesh.vertices[0].normal,
                before.mesh.vertices[0].normal
            );
            assert_eq!(after.mesh.vertices[0].uv, before.mesh.vertices[0].uv);
            assert_eq!(after.mesh.vertices[0].color, before.mesh.vertices[0].color);
            assert_eq!(after.mesh.indices, before.mesh.indices);
            assert_eq!(after.bounds_min, before.bounds_min);
            assert_eq!(after.bounds_max, before.bounds_max);
            assert_eq!(after.center, before.center);
            assert_eq!(after.name, before.name);
            assert_eq!(after.path, before.path);
            assert_eq!(after.node, before.node);
            assert_eq!(after.surface.alpha_cutoff, before.surface.alpha_cutoff);
            assert_eq!(turned.bounds_min, model.bounds_min);
            assert_eq!(turned.bounds_max, model.bounds_max);
            assert_eq!(turned.stats.triangles, model.stats.triangles);
            assert_eq!(turned.root_names, model.root_names);
        }
        assert_eq!(crate::camera::Camera3D::default().up, Vec3::Z);
    }

    #[test]
    fn z_up_rotates_positions_normals_and_the_combined_bounds() {
        let model = model_from_parts(vec![
            part_with_vertices(
                vec![vertex_at(
                    Vec3::new(0.0, 2.0, 0.0),
                    glam::vec4(0.0, 1.0, 0.0, 0.5),
                )],
                vec![0],
                "post",
            ),
            part_with_vertices(
                vec![vertex_at(
                    Vec3::new(3.0, 0.0, -1.0),
                    glam::vec4(1.0, 0.0, 0.0, 0.25),
                )],
                vec![0],
                "peg",
            ),
        ]);
        let rotation = Quat::from_rotation_arc(Vec3::Y, Vec3::Z);
        let turned = model.with_up_axis(Vec3::Z);
        assert_eq!(
            model.parts[0].mesh.vertices[0].position,
            Vec3::new(0.0, 2.0, 0.0),
            "the source model stays put"
        );
        for (before, after) in model.parts.iter().zip(&turned.parts) {
            let from = &before.mesh.vertices[0];
            let to = &after.mesh.vertices[0];
            close(to.position, rotation * from.position);
            close(to.normal.truncate(), rotation * from.normal.truncate());
            assert_eq!(to.normal.w, from.normal.w);
            assert_eq!(to.uv, from.uv);
            assert_eq!(to.color, from.color);
            assert_eq!(after.mesh.indices, before.mesh.indices);
            assert_eq!(after.path, before.path);
            assert_eq!(after.surface.name, before.surface.name);
            close(after.center, after.bounds_min);
            close(after.bounds_max, after.bounds_min);
        }
        close(
            turned.parts[0].mesh.vertices[0].position,
            Vec3::new(0.0, 0.0, 2.0),
        );
        close(turned.parts[0].mesh.vertices[0].normal.truncate(), Vec3::Z);
        close(
            turned.parts[1].mesh.vertices[0].position,
            Vec3::new(3.0, 1.0, 0.0),
        );
        close(turned.bounds_min, Vec3::new(0.0, 0.0, 0.0));
        close(turned.bounds_max, Vec3::new(3.0, 1.0, 2.0));
        assert_eq!(turned.stats.triangles, model.stats.triangles);
        assert_eq!(turned.stats.draw_calls, model.stats.draw_calls);
        assert_eq!(turned.root_names, model.root_names);

        let along_x = model.with_up_axis(Vec3::X);
        close(
            along_x.parts[0].mesh.vertices[0].position,
            Vec3::new(2.0, 0.0, 0.0),
        );
        let flipped = model.with_up_axis(-Vec3::Y);
        close(
            flipped.parts[0].mesh.vertices[0].position,
            Vec3::new(0.0, -2.0, 0.0),
        );
        let diagonal = model.with_up_axis(Vec3::new(0.0, 1.0, 1.0));
        close(
            diagonal.parts[0].mesh.vertices[0].position,
            Vec3::new(0.0, 1.0, 1.0).normalize() * 2.0,
        );
    }

    #[test]
    fn empty_chunks_rotate_their_stored_bounds() {
        let empty = GltfModel {
            parts: Vec::new(),
            stats: GltfModelStats::default(),
            bounds_min: Vec3::ZERO,
            bounds_max: Vec3::ZERO,
            root_names: Vec::new(),
        };
        let turned_empty = empty.with_up_axis(Vec3::Z);
        assert!(turned_empty.parts.is_empty());
        assert_eq!(turned_empty.bounds_min, Vec3::ZERO);
        assert_eq!(turned_empty.bounds_max, Vec3::ZERO);

        let mut gap = part_with_vertices(Vec::new(), Vec::new(), "gap");
        gap.bounds_min = Vec3::new(0.0, 2.0, 0.0);
        gap.bounds_max = Vec3::new(1.0, 4.0, 0.0);
        gap.center = Vec3::new(0.5, 3.0, 0.0);
        let model = model_from_parts(vec![gap]);
        let turned = model.with_up_axis(Vec3::Z);
        close(turned.parts[0].bounds_min, Vec3::new(0.0, 0.0, 2.0));
        close(turned.parts[0].bounds_max, Vec3::new(1.0, 0.0, 4.0));
        close(turned.parts[0].center, Vec3::new(0.5, 0.0, 3.0));
        close(turned.bounds_min, turned.parts[0].bounds_min);
        close(turned.bounds_max, turned.parts[0].bounds_max);
        assert!(turned.parts[0].mesh.vertices.is_empty());
    }

    #[test]
    fn mirrored_winding_stays_put_when_the_up_axis_changes() {
        let primitive = GltfPrimitive {
            positions: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
            normals: vec![Vec3::Z; 3],
            uvs: vec![
                Vec2::new(0.1, 0.2),
                Vec2::new(0.3, 0.4),
                Vec2::new(0.5, 0.6),
            ],
            indices: vec![0, 1, 2],
            base_color_factor: [0.2, 0.4, 0.6, 1.0],
            base_color_texture: None,
        };
        let world = Mat4::from_scale(Vec3::new(-2.0, 1.0, 1.0));
        let vertices = world_vertices(&primitive, world);
        let indices = winding_indices(&primitive.indices, world);
        assert_eq!(indices.as_ref(), &[0, 2, 1]);
        let model = model_from_parts(vec![part_with_vertices(
            vertices,
            indices.iter().map(|index| *index as u16).collect(),
            "mirror",
        )]);
        let rotation = Quat::from_rotation_arc(Vec3::Y, Vec3::Z);
        let turned = model.with_up_axis(Vec3::Z);
        assert_eq!(turned.parts[0].mesh.indices, model.parts[0].mesh.indices);
        let expected = world_vertices(&primitive, Mat4::from_quat(rotation) * world);
        for (got, expect) in turned.parts[0].mesh.vertices.iter().zip(&expected) {
            close(got.position, expect.position);
            close(got.normal.truncate(), expect.normal.truncate());
            assert_eq!(got.uv, expect.uv);
        }
        let index = &turned.parts[0].mesh.indices;
        let verts = &turned.parts[0].mesh.vertices;
        let a = verts[index[0] as usize].position;
        let b = verts[index[1] as usize].position;
        let c = verts[index[2] as usize].position;
        assert!(
            (b - a)
                .cross(c - a)
                .dot(verts[index[0] as usize].normal.truncate())
                > 0.0
        );
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

    #[test]
    fn loaded_model_matches_the_scene_up_axis() {
        let positions = [Vec3::new(0.0, 1.0, 0.0), Vec3::X, Vec3::new(0.0, 0.0, 1.0)];
        let normal = Vec3::new(1.0, 1.0, 0.0).normalize();
        let mut bin = Vec::new();
        for position in positions {
            for value in [position.x, position.y, position.z] {
                bin.extend(value.to_le_bytes());
            }
        }
        for _ in 0..3 {
            for value in [normal.x, normal.y, normal.z] {
                bin.extend(value.to_le_bytes());
            }
        }
        let json = r#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0,2,3]}],"nodes":[{"name":"root","translation":[0,5,0],"children":[1]},{"name":"post","translation":[1,0,0],"scale":[2,1,1],"mesh":0},{"name":"player_spawn","translation":[0,4,1]},{"name":"floor_collider","translation":[0,1,2],"scale":[2,0.2,4],"extras":{"mq_shape":"box","mq_role":"collider"}}],"meshes":[{"primitives":[{"attributes":{"POSITION":0,"NORMAL":1}}]}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,1]},{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":36}],"buffers":[{"byteLength":72}]}"#;
        let scene = crate::gltf::GltfScene::from_bytes(&glb(json, &bin)).unwrap();
        assert_eq!(scene.nodes[1].name.as_deref(), Some("post"));
        assert_eq!(scene.spawns.len(), 1);
        assert_eq!(scene.colliders.len(), 1);
        let primitive = &scene.meshes[scene.nodes[1].mesh.unwrap()].primitives[0];
        let baked = world_vertices(primitive, scene.nodes[1].world);
        let model = model_from_parts(vec![part_with_vertices(
            baked,
            primitive
                .indices
                .iter()
                .map(|index| *index as u16)
                .collect(),
            "post",
        )]);
        let same = model.with_up_axis(Vec3::Y);
        assert_eq!(
            same.parts[0].mesh.vertices[0].position,
            model.parts[0].mesh.vertices[0].position
        );

        let rotation = Quat::from_rotation_arc(Vec3::Y, Vec3::Z);
        let turned_model = model.with_up_axis(Vec3::Z);
        let turned_scene = scene.with_up_axis(Vec3::Z);
        let expected = world_vertices(primitive, turned_scene.nodes[1].world);
        assert_eq!(turned_model.parts[0].mesh.vertices.len(), expected.len());
        for (got, expect) in turned_model.parts[0].mesh.vertices.iter().zip(&expected) {
            close(got.position, expect.position);
            close(got.normal.truncate(), expect.normal.truncate());
        }
        for (before, after) in model.parts[0]
            .mesh
            .vertices
            .iter()
            .zip(&turned_model.parts[0].mesh.vertices)
        {
            close(after.position, rotation * before.position);
            close(after.normal.truncate(), rotation * before.normal.truncate());
        }
        close(
            turned_scene.spawns[0].position,
            rotation * scene.spawns[0].position,
        );
        close(
            turned_scene.nodes[1].world.transform_point3(positions[0]),
            turned_model.parts[0].mesh.vertices[0].position,
        );
        let crate::gltf::GltfColliderShape::Aabb(floor) = turned_scene.colliders[0].shape else {
            panic!("quarter turn keeps the floor an Aabb");
        };
        close(floor.center(), rotation * Vec3::new(0.0, 1.0, 2.0));
        assert_eq!(
            turned_model.parts[0].mesh.indices,
            model.parts[0].mesh.indices
        );
        assert_eq!(crate::camera::Camera3D::default().up, Vec3::Z);
    }
}
