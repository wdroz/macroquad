//! GPU copies of normal and metallic-roughness maps.
//!
//! The textures live here, not on [`super::GltfDrawMesh`], so existing draw-mesh
//! struct literals stay valid. [`super::GltfMaterialFrame::submit_shaded`] samples
//! them; [`super::GltfMaterialFrame::submit`] does not.

use glam::{Vec2, Vec3, Vec4};

use crate::gltf::GltfUvTransform;
use crate::texture::Texture2D;

/// One uploaded normal or metallic-roughness map for a draw chunk.
///
/// `uvs` matches [`super::GltfDrawMesh::mesh`] vertex order and already includes
/// [`GltfUvTransform`]. The handle is shared with every other use of that glTF
/// texture, including a base-color image that points at the same texture.
/// Normal and metallic-roughness bytes are linear data as stored in the file;
/// this upload does not mark them as sRGB.
///
/// `texture` is a [`Texture2D`](crate::texture::Texture2D). Constructing one
/// needs a running macroquad window, so this example only shows the UV
/// transform stored beside it.
///
/// ```
/// use macroquad::gltf::GltfUvTransform;
/// use macroquad::math::vec2;
///
/// let transform = GltfUvTransform::default();
/// assert_eq!(transform.apply(vec2(0.25, 0.5)), vec2(0.25, 0.5));
/// ```
#[derive(Clone)]
pub struct GltfDrawMaterialMap {
    /// Shared GPU image.
    pub texture: Texture2D,
    /// `TEXCOORD_n` after a transform override.
    pub tex_coord: u32,
    /// Offset, rotation, and scale already applied to [`Self::uvs`].
    pub transform: GltfUvTransform,
    /// `normalTexture.scale`, or `1` for metallic-roughness.
    pub strength: f32,
    /// Transformed UVs, one per chunk vertex.
    pub uvs: Vec<Vec2>,
}

/// Maps and world-space tangents for one [`super::GltfDrawMesh`].
///
/// `tangents` is empty or the same length as the chunk's vertices. `xyz` is
/// the tangent after the node matrix (inverse transpose, then normalized) and
/// `w` is the glTF handedness. A mirrored node flips `w`. [`GltfModel::with_up_axis`](super::GltfModel::with_up_axis)
/// does not rotate this value; use [`GltfModelMaterialMaps::with_up_axis`] with
/// the same axis.
///
/// ```
/// use macroquad::gltf_draw::GltfPartMaterialMaps;
/// use macroquad::math::vec4;
///
/// let part = GltfPartMaterialMaps {
///     normal: None,
///     metallic_roughness: None,
///     tangents: vec![vec4(1.0, 0.0, 0.0, 1.0)],
///     material_index: None,
///     material_name: None,
/// };
/// assert_eq!(part.tangents.len(), 1);
/// ```
#[derive(Clone)]
pub struct GltfPartMaterialMaps {
    /// Normal map for this chunk, or [`None`].
    pub normal: Option<GltfDrawMaterialMap>,
    /// Metallic-roughness map for this chunk, or [`None`].
    pub metallic_roughness: Option<GltfDrawMaterialMap>,
    /// World-space tangents in chunk vertex order. See the struct docs.
    pub tangents: Vec<Vec4>,
    /// Source material index, or [`None`] for the default material.
    pub material_index: Option<usize>,
    /// Source material name, or [`None`].
    pub material_name: Option<String>,
}

/// Normal and metallic-roughness data parallel to [`super::GltfModel::parts`].
///
/// [`super::GltfModel::from_bytes`] validates these maps and then drops the
/// handles. [`super::GltfModel::from_bytes_with_material_maps`] and
/// [`super::GltfModelLoader::take_material_maps`] keep them. Shading-only
/// textures are not included in [`super::GltfModelStats::textures`] and are not
/// listed by [`super::GltfModel::resource_report`] unless that same texture is
/// also a base-color or emissive image on the mesh.
///
/// ```
/// use macroquad::gltf_draw::GltfModelMaterialMaps;
///
/// let maps = GltfModelMaterialMaps { parts: Vec::new() };
/// assert!(maps.with_up_axis(macroquad::math::Vec3::Y).parts.is_empty());
/// ```
#[derive(Clone)]
pub struct GltfModelMaterialMaps {
    /// One entry per draw chunk, in [`super::GltfModel::parts`] order.
    pub parts: Vec<GltfPartMaterialMaps>,
}

impl GltfModelMaterialMaps {
    /// Rotates tangent directions by the same rule as [`super::GltfModel::with_up_axis`].
    ///
    /// [`macroquad::math::Vec3::Y`] returns a clone. Handedness is unchanged
    /// because this rotation is not a mirror. UVs and textures stay put.
    ///
    /// ```
    /// use macroquad::gltf_draw::GltfModelMaterialMaps;
    /// use macroquad::math::{vec4, Vec3};
    /// use macroquad::gltf_draw::GltfPartMaterialMaps;
    ///
    /// let maps = GltfModelMaterialMaps {
    ///     parts: vec![GltfPartMaterialMaps {
    ///         normal: None,
    ///         metallic_roughness: None,
    ///         tangents: vec![vec4(0.0, 1.0, 0.0, -1.0)],
    ///         material_index: Some(0),
    ///         material_name: Some("paint".into()),
    ///     }],
    /// };
    /// let turned = maps.with_up_axis(Vec3::Z);
    /// assert!((turned.parts[0].tangents[0].truncate() - Vec3::Z).length() < 1.0e-4);
    /// assert_eq!(turned.parts[0].tangents[0].w, -1.0);
    /// ```
    pub fn with_up_axis(&self, up: Vec3) -> Self {
        let Some(rotation) = crate::gltf::up_axis_rotation(up) else {
            return self.clone();
        };
        let mut maps = self.clone();
        for part in &mut maps.parts {
            for tangent in &mut part.tangents {
                let xyz = rotation * tangent.truncate();
                *tangent = xyz.extend(tangent.w);
            }
        }
        maps
    }
}

/// A draw model and the shading maps that are not fields of [`super::GltfModel`].
///
/// ```no_run
/// use macroquad::gltf_draw::{GltfLoadSettings, GltfModel};
/// # fn load(bytes: &[u8]) -> Result<(), String> {
/// let assets = GltfModel::from_bytes_with_material_maps(bytes, GltfLoadSettings::default())?;
/// assert_eq!(assets.material_maps.parts.len(), assets.model.parts.len());
/// # Ok(()) }
/// ```
#[derive(Clone)]
pub struct GltfModelAssets {
    /// Textured chunks. Base color and emissive images stay on the mesh.
    pub model: super::GltfModel,
    /// Normal and metallic-roughness maps, one entry per chunk.
    pub material_maps: GltfModelMaterialMaps,
}
