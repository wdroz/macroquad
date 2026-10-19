//! Normal and metallic-roughness images, UV mappings, and tangents.
//!
//! These stay off [`super::GltfScene`] and [`super::GltfPrimitive`] so existing
//! struct literals keep compiling. [`GltfMaterialMaps`] retains the decoded
//! images even though the material renderer does not sample them yet.

use std::collections::HashMap;

use glam::{Mat4, Vec2, Vec3, Vec4};

use super::{image_bytes, image_context, load_buffers, used_meshes, GltfError, GltfImage};
use crate::gltf::GltfImageLimits;

/// UV affine transform from `KHR_texture_transform`.
///
/// glTF applies this as `offset + rotate(uv * scale)`. The rotation is radians
/// around the origin. The default is no offset, no rotation, and a scale of 1.
///
/// ```
/// use macroquad::gltf::GltfUvTransform;
/// use macroquad::math::vec2;
///
/// let transform = GltfUvTransform {
///     offset: vec2(0.5, 0.25),
///     rotation: 0.0,
///     scale: vec2(-1.0, 1.0),
/// };
/// assert_eq!(transform.apply(vec2(1.0, 0.0)), vec2(-0.5, 0.25));
/// assert_eq!(GltfUvTransform::default().apply(vec2(0.2, 0.4)), vec2(0.2, 0.4));
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GltfUvTransform {
    /// Added after scale and rotation.
    pub offset: Vec2,
    /// Counter-clockwise rotation in radians.
    pub rotation: f32,
    /// Multiplied with the UV before rotation.
    pub scale: Vec2,
}

impl Default for GltfUvTransform {
    fn default() -> Self {
        Self {
            offset: Vec2::ZERO,
            rotation: 0.0,
            scale: Vec2::ONE,
        }
    }
}

impl GltfUvTransform {
    /// Returns `offset + rotate(uv * scale)`.
    ///
    /// ```
    /// use macroquad::gltf::GltfUvTransform;
    /// use macroquad::math::vec2;
    ///
    /// let turned = GltfUvTransform {
    ///     rotation: std::f32::consts::FRAC_PI_2,
    ///     ..GltfUvTransform::default()
    /// };
    /// let uv = turned.apply(vec2(1.0, 0.0));
    /// assert!((uv - vec2(0.0, 1.0)).length() < 1.0e-5);
    /// ```
    pub fn apply(self, uv: Vec2) -> Vec2 {
        let uv = uv * self.scale;
        let (sin, cos) = self.rotation.sin_cos();
        self.offset + Vec2::new(cos * uv.x - sin * uv.y, sin * uv.x + cos * uv.y)
    }
}

/// One normal or metallic-roughness binding for a mesh primitive.
///
/// `uvs` are the selected texture-coordinate set after [`GltfUvTransform`],
/// one value per position, in mesh-local order. They are not rewritten by the
/// node's world matrix. `image` indexes [`GltfMaterialMaps::images`].
///
/// ```
/// use macroquad::gltf::{GltfMaterialMap, GltfUvTransform};
/// use macroquad::math::vec2;
///
/// let map = GltfMaterialMap {
///     image: 0,
///     tex_coord: 1,
///     transform: GltfUvTransform::default(),
///     strength: 1.0,
///     uvs: vec![vec2(0.0, 0.0)],
/// };
/// assert_eq!(map.tex_coord, 1);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct GltfMaterialMap {
    /// Index into [`GltfMaterialMaps::images`].
    pub image: usize,
    /// `TEXCOORD_n` after a `KHR_texture_transform` override, when present.
    pub tex_coord: u32,
    /// Offset, rotation, and scale. [`GltfUvTransform::default`] when the
    /// extension is absent.
    pub transform: GltfUvTransform,
    /// `normalTexture.scale`, or `1` for a metallic-roughness map.
    pub strength: f32,
    /// Transformed UVs, parallel to the primitive's positions.
    pub uvs: Vec<Vec2>,
}

/// Maps and tangents for one primitive.
///
/// Tangents are mesh-local. `xyz` is the tangent direction and `w` is the
/// glTF handedness (`bitangent = cross(normal, tangent) * w`). An authored
/// `TANGENT` accessor is kept, including a negative `w`. When a normal map is
/// present and that accessor is absent, tangents are generated from the normal
/// map's transformed UVs, so a mirrored UV scale produces `w == -1`.
/// Metallic-roughness alone does not generate tangents. The list is empty when
/// there is neither an accessor nor a normal map.
///
/// ```
/// use macroquad::gltf::GltfPrimitiveMaterialMaps;
/// use macroquad::math::vec4;
///
/// let primitive = GltfPrimitiveMaterialMaps {
///     normal: None,
///     metallic_roughness: None,
///     tangents: vec![vec4(1.0, 0.0, 0.0, -1.0)],
///     material_index: Some(0),
///     material_name: Some("paint".into()),
/// };
/// assert_eq!(primitive.tangents[0].w, -1.0);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct GltfPrimitiveMaterialMaps {
    /// Normal map, or [`None`] when the material has none.
    pub normal: Option<GltfMaterialMap>,
    /// Metallic-roughness map, or [`None`] when the material has none.
    ///
    /// glTF stores occlusion in R, roughness in G, and metallic in B. The bytes
    /// are the decoded image, not converted to another color space.
    pub metallic_roughness: Option<GltfMaterialMap>,
    /// Mesh-local tangents. See the struct docs.
    pub tangents: Vec<Vec4>,
    /// Index in the file's materials array, or [`None`] for the default material.
    pub material_index: Option<usize>,
    /// Material name, or [`None`] when it has none.
    pub material_name: Option<String>,
}

/// Decoded normal and metallic-roughness images for one document.
///
/// [`super::GltfScene::images`] stays the base-color list. Images here are
/// shared: two primitives or both slots that sample one glTF image get one
/// [`GltfImage`] and the same index. Occlusion maps are not loaded. An external
/// image URI is an error only when a loaded primitive samples it.
///
/// `meshes` matches [`super::GltfScene::meshes`] for the same bytes: one entry
/// per referenced mesh, in first-use order, then one entry per primitive.
/// Node instances share that mesh entry. A mirrored node does not change these
/// mesh-local tangents; the draw loader bakes that into
/// [`crate::gltf_draw::GltfModelMaterialMaps`].
///
/// ```
/// use macroquad::gltf::GltfMaterialMaps;
///
/// let maps = GltfMaterialMaps::from_bytes(br#"{"asset":{"version":"2.0"}}"#).unwrap();
/// assert!(maps.images.is_empty());
/// assert!(maps.meshes.is_empty());
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct GltfMaterialMaps {
    /// Shared normal and metallic-roughness images, in first-use order.
    pub images: Vec<GltfImage>,
    /// Parallel to [`super::GltfScene::meshes`].
    pub meshes: Vec<Vec<GltfPrimitiveMaterialMaps>>,
}

impl GltfMaterialMaps {
    /// Load normal and metallic-roughness maps from a `.glb` or `.gltf` slice.
    ///
    /// No window and no GPU textures. See [`Self::from_bytes_with_image_limits`]
    /// to reject an oversized source image before decoding.
    ///
    /// ```
    /// use macroquad::gltf::GltfMaterialMaps;
    ///
    /// let maps = GltfMaterialMaps::from_bytes(
    ///     br#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[]}]}"#,
    /// )
    /// .unwrap();
    /// assert!(maps.meshes.is_empty());
    /// ```
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, GltfError> {
        Self::from_bytes_with_image_limits(bytes, GltfImageLimits::default())
    }

    /// [`Self::from_bytes`] with the same per-image limits as the other loaders.
    ///
    /// Every normal and metallic-roughness image sampled by the selected scene
    /// is checked before any of those images is decoded. Unused images,
    /// occlusion maps, and base-color images are ignored here. The limits do
    /// not bound decoder scratch memory.
    ///
    /// ```
    /// use macroquad::gltf::{GltfImageLimits, GltfMaterialMaps};
    ///
    /// let maps = GltfMaterialMaps::from_bytes_with_image_limits(
    ///     br#"{"asset":{"version":"2.0"}}"#,
    ///     GltfImageLimits {
    ///         max_source_pixels: Some(64),
    ///         max_decoded_bytes: Some(256),
    ///     },
    /// )
    /// .unwrap();
    /// assert!(maps.images.is_empty());
    /// ```
    pub fn from_bytes_with_image_limits(
        bytes: &[u8],
        image_limits: GltfImageLimits,
    ) -> Result<Self, GltfError> {
        if bytes.starts_with(b"glTF") {
            let glb = gltf::binary::Glb::from_slice(bytes).map_err(|error| {
                GltfError::new(format!("glTF document could not be parsed: {error}"))
            })?;
            let gltf = gltf::Gltf::from_slice(&glb.json).map_err(|error| {
                GltfError::new(format!("glTF document could not be parsed: {error}"))
            })?;
            Self::from_document(&gltf.document, glb.bin.as_deref(), image_limits)
        } else {
            let gltf = gltf::Gltf::from_slice(bytes).map_err(|error| {
                GltfError::new(format!("glTF document could not be parsed: {error}"))
            })?;
            Self::from_document(&gltf.document, None, image_limits)
        }
    }

    fn from_document(
        document: &gltf::Document,
        binary: Option<&[u8]>,
        image_limits: GltfImageLimits,
    ) -> Result<Self, GltfError> {
        let buffers = load_buffers(document, binary)?;
        let source_meshes = used_meshes(document);
        if source_meshes.is_empty() {
            return Ok(Self {
                images: Vec::new(),
                meshes: Vec::new(),
            });
        }
        let mut built = Vec::new();
        let mut image_jobs: Vec<(usize, String)> = Vec::new();
        let mut seen_images = HashMap::<usize, usize>::new();
        for mesh in &source_meshes {
            let mut primitives = Vec::new();
            for primitive in mesh.primitives() {
                let data = resolve_primitive_maps(&primitive, &|index| {
                    buffers.get(index).map(|data| data.as_ref())
                })
                .map_err(GltfError::new)?;
                remember_image(
                    document,
                    &mut image_jobs,
                    &mut seen_images,
                    data.normal.as_ref(),
                    &primitive.material(),
                    "normal",
                );
                remember_image(
                    document,
                    &mut image_jobs,
                    &mut seen_images,
                    data.metallic_roughness.as_ref(),
                    &primitive.material(),
                    "metallic-roughness",
                );
                primitives.push(data);
            }
            built.push(primitives);
        }
        if image_limits != GltfImageLimits::default() {
            for (image_index, context) in &image_jobs {
                let image = document.images().nth(*image_index).unwrap();
                let data = image_bytes(&image, &buffers)
                    .map_err(|error| GltfError::new(format!("{context}: {error}")))?;
                super::image_limits::check_image(&data, image_limits)
                    .map_err(|error| GltfError::new(format!("{context}: {error}")))?;
            }
        }
        let mut images = Vec::new();
        let mut image_of = HashMap::new();
        for (image_index, context) in &image_jobs {
            if image_of.contains_key(image_index) {
                continue;
            }
            let image = document.images().nth(*image_index).unwrap();
            let bytes = image_bytes(&image, &buffers)
                .map_err(|error| GltfError::new(format!("{context}: {error}")))?;
            let decoded = image::load_from_memory(&bytes).map_err(|error| {
                GltfError::new(format!("{context} could not be decoded: {error}"))
            })?;
            let rgba = decoded.into_rgba8();
            let index = images.len();
            images.push(GltfImage {
                name: image.name().map(str::to_owned),
                width: rgba.width(),
                height: rgba.height(),
                pixels: rgba.into_raw(),
            });
            image_of.insert(*image_index, index);
        }
        let meshes = built
            .into_iter()
            .map(|primitives| {
                primitives
                    .into_iter()
                    .map(|data| GltfPrimitiveMaterialMaps {
                        normal: data.normal.map(|map| public_map(map, &image_of)),
                        metallic_roughness: data
                            .metallic_roughness
                            .map(|map| public_map(map, &image_of)),
                        tangents: data.tangents,
                        material_index: data.material_index,
                        material_name: data.material_name,
                    })
                    .collect()
            })
            .collect();
        Ok(Self { images, meshes })
    }
}

fn public_map(map: ResolvedMap, image_of: &HashMap<usize, usize>) -> GltfMaterialMap {
    GltfMaterialMap {
        image: image_of[&map.source_image],
        tex_coord: map.tex_coord,
        transform: map.transform,
        strength: map.strength,
        uvs: map.uvs,
    }
}

fn remember_image(
    document: &gltf::Document,
    jobs: &mut Vec<(usize, String)>,
    seen: &mut HashMap<usize, usize>,
    map: Option<&ResolvedMap>,
    material: &gltf::Material<'_>,
    slot: &str,
) {
    let Some(map) = map else {
        return;
    };
    if seen.insert(map.source_image, jobs.len()).is_some() {
        return;
    }
    let image = document.images().nth(map.source_image).unwrap();
    jobs.push((map.source_image, image_context(&image, material, slot)));
}

pub(crate) struct ResolvedMap {
    pub texture_index: usize,
    pub source_image: usize,
    pub tex_coord: u32,
    pub transform: GltfUvTransform,
    pub strength: f32,
    pub uvs: Vec<Vec2>,
}

pub(crate) struct PrimitiveMapData {
    pub normal: Option<ResolvedMap>,
    pub metallic_roughness: Option<ResolvedMap>,
    pub tangents: Vec<Vec4>,
    pub material_index: Option<usize>,
    pub material_name: Option<String>,
}

/// Mesh-local maps for one primitive.
///
/// `positions` overrides the accessor read when the caller already has them.
/// Passing [`None`] reads `POSITION` from the primitive.
pub(crate) fn resolve_primitive_maps<'b>(
    primitive: &gltf::Primitive<'_>,
    get_buffer: &dyn Fn(usize) -> Option<&'b [u8]>,
) -> Result<PrimitiveMapData, String> {
    if primitive.mode() != gltf::mesh::Mode::Triangles {
        return Err(format!(
            "glTF primitive {} uses {:?}; only triangles are loaded",
            primitive.index(),
            primitive.mode()
        ));
    }
    let material = primitive.material();
    let label = material_label(&material);
    let reader = primitive.reader(|buffer| get_buffer(buffer.index()));
    let positions: Vec<Vec3> = reader
        .read_positions()
        .ok_or_else(|| format!("{label}: primitive has no positions"))?
        .map(Vec3::from)
        .collect();
    let normals: Vec<Vec3> = reader
        .read_normals()
        .map(|normals| normals.map(Vec3::from).collect())
        .unwrap_or_default();
    if !(normals.is_empty() || normals.len() == positions.len()) {
        return Err(format!(
            "{label}: {} normals for {} positions",
            normals.len(),
            positions.len()
        ));
    }
    let indices: Vec<u32> = match reader.read_indices() {
        Some(indices) => indices.into_u32().collect(),
        None => (0..positions.len() as u32).collect(),
    };
    if indices
        .iter()
        .any(|index| *index as usize >= positions.len())
    {
        return Err(format!(
            "{label}: an index is outside its {} positions",
            positions.len()
        ));
    }
    let normal = match material.normal_texture() {
        Some(info) => Some(read_binding(
            primitive,
            get_buffer,
            info.texture().index(),
            info.texture().source().index(),
            info.tex_coord(),
            info.scale(),
            info.extension_value("KHR_texture_transform"),
            &format!("{label} normal map"),
            positions.len(),
        )?),
        None => None,
    };
    let metallic_roughness = match material
        .pbr_metallic_roughness()
        .metallic_roughness_texture()
    {
        Some(info) => Some(read_binding(
            primitive,
            get_buffer,
            info.texture().index(),
            info.texture().source().index(),
            info.tex_coord(),
            1.0,
            info.extension_value("KHR_texture_transform"),
            &format!("{label} metallic-roughness map"),
            positions.len(),
        )?),
        None => None,
    };
    let tangents = if let Some(tangents) = reader.read_tangents() {
        let tangents: Vec<Vec4> = tangents.map(Vec4::from).collect();
        if tangents.len() != positions.len() {
            return Err(format!(
                "{label}: TANGENT has {} values for {} positions",
                tangents.len(),
                positions.len()
            ));
        }
        if tangents.iter().any(|tangent| !tangent.is_finite()) {
            return Err(format!("{label}: TANGENT has a non-finite value"));
        }
        tangents
    } else if let Some(normal) = &normal {
        generate_tangents(&positions, &normals, &normal.uvs, &indices)
    } else {
        Vec::new()
    };
    Ok(PrimitiveMapData {
        normal,
        metallic_roughness,
        tangents,
        material_index: material.index(),
        material_name: material.name().map(str::to_owned),
    })
}

fn read_binding<'b>(
    primitive: &gltf::Primitive<'_>,
    get_buffer: &dyn Fn(usize) -> Option<&'b [u8]>,
    texture_index: usize,
    source_image: usize,
    tex_coord: u32,
    strength: f32,
    transform: Option<&serde_json::Value>,
    label: &str,
    position_count: usize,
) -> Result<ResolvedMap, String> {
    if !strength.is_finite() {
        return Err(format!("{label}: strength is not finite"));
    }
    let (tex_coord, transform) = parse_uv_transform(transform, tex_coord, label)?;
    let reader = primitive.reader(|buffer| get_buffer(buffer.index()));
    let coords = reader
        .read_tex_coords(tex_coord)
        .ok_or_else(|| format!("{label}: missing TEXCOORD_{tex_coord}"))?;
    let raw: Vec<Vec2> = coords.into_f32().map(Vec2::from).collect();
    if raw.len() != position_count {
        return Err(format!(
            "{label}: TEXCOORD_{tex_coord} has {} values for {position_count} positions",
            raw.len()
        ));
    }
    if raw.iter().any(|uv| !uv.is_finite()) {
        return Err(format!(
            "{label}: TEXCOORD_{tex_coord} has a non-finite value"
        ));
    }
    let uvs = raw.into_iter().map(|uv| transform.apply(uv)).collect();
    Ok(ResolvedMap {
        texture_index,
        source_image,
        tex_coord,
        transform,
        strength,
        uvs,
    })
}

fn parse_uv_transform(
    value: Option<&serde_json::Value>,
    tex_coord: u32,
    label: &str,
) -> Result<(u32, GltfUvTransform), String> {
    let Some(value) = value else {
        return Ok((tex_coord, GltfUvTransform::default()));
    };
    if !value.is_object() {
        return Err(format!("{label}: KHR_texture_transform is not an object"));
    }
    let offset = optional_vec2(value, "offset", Vec2::ZERO, label)?;
    let scale = optional_vec2(value, "scale", Vec2::ONE, label)?;
    let rotation = optional_f32(value, "rotation", 0.0, label)?;
    let tex_coord = match value.get("texCoord") {
        None => tex_coord,
        Some(tex_coord) => {
            let tex_coord = tex_coord.as_u64().ok_or_else(|| {
                format!("{label}: KHR_texture_transform texCoord is not an unsigned integer")
            })?;
            u32::try_from(tex_coord).map_err(|_| {
                format!("{label}: KHR_texture_transform texCoord does not fit in u32")
            })?
        }
    };
    if !offset.is_finite() || !scale.is_finite() || !rotation.is_finite() {
        return Err(format!(
            "{label}: KHR_texture_transform values must be finite"
        ));
    }
    Ok((
        tex_coord,
        GltfUvTransform {
            offset,
            rotation,
            scale,
        },
    ))
}

fn optional_vec2(
    value: &serde_json::Value,
    name: &str,
    default: Vec2,
    label: &str,
) -> Result<Vec2, String> {
    match value.get(name) {
        None => Ok(default),
        Some(component) => {
            let component = component
                .as_array()
                .filter(|component| component.len() == 2)
                .ok_or_else(|| {
                    format!("{label}: KHR_texture_transform {name} is not a 2-component array")
                })?;
            let x = component[0].as_f64().ok_or_else(|| {
                format!("{label}: KHR_texture_transform {name} is not a pair of numbers")
            })?;
            let y = component[1].as_f64().ok_or_else(|| {
                format!("{label}: KHR_texture_transform {name} is not a pair of numbers")
            })?;
            Ok(Vec2::new(x as f32, y as f32))
        }
    }
}

fn optional_f32(
    value: &serde_json::Value,
    name: &str,
    default: f32,
    label: &str,
) -> Result<f32, String> {
    match value.get(name) {
        None => Ok(default),
        Some(component) => component
            .as_f64()
            .map(|component| component as f32)
            .ok_or_else(|| format!("{label}: KHR_texture_transform {name} is not a number")),
    }
}

pub(crate) fn material_label(material: &gltf::Material<'_>) -> String {
    let index = material
        .index()
        .map(|index| index.to_string())
        .unwrap_or_else(|| "default".into());
    match material.name() {
        Some(name) => format!("material {index} ({name})"),
        None => format!("material {index}"),
    }
}

pub(crate) fn generate_tangents(
    positions: &[Vec3],
    normals: &[Vec3],
    uvs: &[Vec2],
    indices: &[u32],
) -> Vec<Vec4> {
    let mut tan1 = vec![Vec3::ZERO; positions.len()];
    let mut tan2 = vec![Vec3::ZERO; positions.len()];
    for triangle in indices.chunks_exact(3) {
        let (i0, i1, i2) = (
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        );
        let (p0, p1, p2) = (positions[i0], positions[i1], positions[i2]);
        let (w0, w1, w2) = (uvs[i0], uvs[i1], uvs[i2]);
        let edge1 = p1 - p0;
        let edge2 = p2 - p0;
        let duv1 = w1 - w0;
        let duv2 = w2 - w0;
        let determinant = duv1.x * duv2.y - duv2.x * duv1.y;
        if !determinant.is_finite() || determinant.abs() <= 1.0e-12 {
            continue;
        }
        let inverse = 1.0 / determinant;
        let sdir = (edge1 * duv2.y - edge2 * duv1.y) * inverse;
        let tdir = (edge2 * duv1.x - edge1 * duv2.x) * inverse;
        tan1[i0] += sdir;
        tan1[i1] += sdir;
        tan1[i2] += sdir;
        tan2[i0] += tdir;
        tan2[i1] += tdir;
        tan2[i2] += tdir;
    }
    let generated_normals;
    let normals = if normals.len() == positions.len() {
        normals
    } else {
        generated_normals = geometric_normals(positions, indices);
        &generated_normals
    };
    (0..positions.len())
        .map(|index| {
            let normal = normals[index].normalize_or_zero();
            let mut tangent = (tan1[index] - normal * normal.dot(tan1[index])).normalize_or_zero();
            if tangent == Vec3::ZERO {
                let helper = if normal.x.abs() < 0.9 {
                    Vec3::X
                } else {
                    Vec3::Y
                };
                tangent = helper.cross(normal).normalize_or_zero();
            }
            let handedness = if normal.cross(tangent).dot(tan2[index]) < 0.0 {
                -1.0
            } else {
                1.0
            };
            tangent.extend(handedness)
        })
        .collect()
}

fn geometric_normals(positions: &[Vec3], indices: &[u32]) -> Vec<Vec3> {
    let mut normals = vec![Vec3::ZERO; positions.len()];
    for triangle in indices.chunks_exact(3) {
        let (i0, i1, i2) = (
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        );
        let normal = (positions[i1] - positions[i0]).cross(positions[i2] - positions[i0]);
        normals[i0] += normal;
        normals[i1] += normal;
        normals[i2] += normal;
    }
    normals
}

/// Transforms mesh-local tangents the same way draw normals are transformed.
///
/// A negative determinant flips `w`. A singular matrix yields zero tangents.
pub(crate) fn world_tangents(local: &[Vec4], world: Mat4) -> Vec<Vec4> {
    if local.is_empty() {
        return Vec::new();
    }
    let normal_matrix = world.inverse().transpose();
    let mirrored = world.determinant() < 0.0;
    local
        .iter()
        .map(|tangent| {
            let xyz = normal_matrix
                .transform_vector3(tangent.truncate())
                .normalize_or_zero();
            let w = if mirrored { -tangent.w } else { tangent.w };
            if xyz.is_finite() && w.is_finite() {
                xyz.extend(w)
            } else {
                Vec4::ZERO
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::vec2;
    use image::ImageEncoder;

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

    fn distinct_uv_fixture() -> Vec<u8> {
        let positions = f32s(&[0., 0., 0., 1., 0., 0., 0., 1., 0.]);
        let normals = f32s(&[0., 0., 1., 0., 0., 1., 0., 0., 1.]);
        let uv0 = f32s(&[0., 0., 1., 0., 0., 1.]);
        let uv1 = f32s(&[0., 0., 0., 1., 1., 0.]);
        let tangents = f32s(&[1., 0., 0., -1., 1., 0., 0., -1., 1., 0., 0., -1.]);
        let indices = [0u16, 1, 2]
            .iter()
            .flat_map(|i| i.to_le_bytes())
            .collect::<Vec<_>>();
        let image = png([10, 20, 30, 255]);
        let mut binary = Vec::new();
        let mut views = Vec::new();
        for chunk in [
            &positions, &normals, &uv0, &uv1, &tangents, &indices, &image,
        ] {
            binary.resize(binary.len().next_multiple_of(4), 0);
            views.push(serde_json::json!({
                "buffer": 0,
                "byteOffset": binary.len(),
                "byteLength": chunk.len(),
            }));
            binary.extend_from_slice(chunk);
        }
        let json = serde_json::json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "extensionsUsed": ["KHR_texture_transform"],
            "scenes": [{"nodes": [0]}],
            "nodes": [{"name": "panel", "mesh": 0}],
            "meshes": [{"primitives": [
                {"attributes": {"POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2, "TEXCOORD_1": 3}, "indices": 5, "material": 0},
                {"attributes": {"POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2, "TEXCOORD_1": 3, "TANGENT": 4}, "indices": 5, "material": 1}
            ]}],
            "materials": [
                {"name": "mirrored", "normalTexture": {"index": 0, "scale": 2.0, "extensions": {"KHR_texture_transform": {"scale": [-1.0, 1.0]}}},
                 "pbrMetallicRoughness": {"metallicRoughnessTexture": {"index": 0, "extensions": {"KHR_texture_transform": {"offset": [0.5, 0.25], "texCoord": 1}}}}},
                {"name": "authored", "normalTexture": {"index": 0}}
            ],
            "textures": [{"source": 0}],
            "images": [{"name": "shared", "bufferView": 6, "mimeType": "image/png"}],
            "buffers": [{"byteLength": binary.len()}],
            "bufferViews": views,
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 0]},
                {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3"},
                {"bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC2"},
                {"bufferView": 3, "componentType": 5126, "count": 3, "type": "VEC2"},
                {"bufferView": 4, "componentType": 5126, "count": 3, "type": "VEC4"},
                {"bufferView": 5, "componentType": 5123, "count": 3, "type": "SCALAR"}
            ]
        });
        glb(json, &binary)
    }

    #[test]
    fn distinct_uvs_share_one_image_and_keep_mirrored_tangents() {
        let bytes = distinct_uv_fixture();
        let maps = GltfMaterialMaps::from_bytes(&bytes).unwrap();
        let scene = super::super::GltfScene::from_bytes(&bytes).unwrap();
        assert!(scene.images.is_empty());
        assert_eq!(maps.meshes.len(), scene.meshes.len());
        assert_eq!(maps.images.len(), 1);
        assert_eq!(maps.images[0].pixels, [10, 20, 30, 255]);
        let mirrored = &maps.meshes[0][0];
        let authored = &maps.meshes[0][1];
        assert_eq!(mirrored.material_name.as_deref(), Some("mirrored"));
        assert_eq!(mirrored.normal.as_ref().unwrap().image, 0);
        assert_eq!(mirrored.metallic_roughness.as_ref().unwrap().image, 0);
        assert_eq!(mirrored.normal.as_ref().unwrap().strength, 2.0);
        assert_eq!(mirrored.normal.as_ref().unwrap().tex_coord, 0);
        assert_eq!(mirrored.metallic_roughness.as_ref().unwrap().tex_coord, 1);
        let normal_uvs = &mirrored.normal.as_ref().unwrap().uvs;
        assert!((normal_uvs[1] - vec2(-1.0, 0.0)).length() < 1.0e-5);
        let rough_uvs = &mirrored.metallic_roughness.as_ref().unwrap().uvs;
        assert!((rough_uvs[0] - vec2(0.5, 0.25)).length() < 1.0e-5);
        assert!((rough_uvs[1] - vec2(0.5, 1.25)).length() < 1.0e-5);
        assert!((rough_uvs[2] - vec2(1.5, 0.25)).length() < 1.0e-5);
        assert!(normal_uvs[1].x != rough_uvs[1].x);
        let tangent = mirrored.tangents[0];
        assert!((tangent.truncate() - Vec3::new(-1.0, 0.0, 0.0)).length() < 1.0e-4);
        assert_eq!(tangent.w, -1.0);
        assert_eq!(authored.tangents[0].w, -1.0);
        assert!((authored.tangents[0].truncate() - Vec3::X).length() < 1.0e-5);
        assert_eq!(authored.normal.as_ref().unwrap().image, 0);
    }

    #[test]
    fn a_missing_uv_set_names_the_material() {
        let mut bytes = distinct_uv_fixture();
        let parsed = gltf::binary::Glb::from_slice(&bytes).unwrap();
        let mut json: serde_json::Value = serde_json::from_slice(&parsed.json).unwrap();
        json["materials"][0]["normalTexture"]["extensions"]["KHR_texture_transform"]["texCoord"] =
            serde_json::json!(3);
        bytes = glb(json, parsed.bin.unwrap().as_ref());
        let error = GltfMaterialMaps::from_bytes(&bytes)
            .unwrap_err()
            .to_string();
        assert!(error.contains("mirrored"), "{error}");
        assert!(error.contains("normal map"), "{error}");
        assert!(error.contains("TEXCOORD_3"), "{error}");
    }

    #[test]
    fn a_malformed_transform_names_the_material_instead_of_dropping_it() {
        let bytes = distinct_uv_fixture();
        let parsed = gltf::binary::Glb::from_slice(&bytes).unwrap();
        let mut json: serde_json::Value = serde_json::from_slice(&parsed.json).unwrap();
        json["materials"][0]["pbrMetallicRoughness"]["metallicRoughnessTexture"]["extensions"]
            ["KHR_texture_transform"]["scale"] = serde_json::json!([1.0]);
        let bytes = glb(json, parsed.bin.unwrap().as_ref());
        let error = GltfMaterialMaps::from_bytes(&bytes)
            .unwrap_err()
            .to_string();
        assert!(error.contains("mirrored"), "{error}");
        assert!(error.contains("metallic-roughness"), "{error}");
        assert!(error.contains("scale"), "{error}");
    }

    #[test]
    fn image_limits_reject_a_normal_map_before_decode_and_ignore_occlusion() {
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
        let json = serde_json::json!({
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
                {"name": "normals", "bufferView": 2, "mimeType": "image/png"},
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
        });
        let bytes = glb(json, &binary);
        let error = GltfMaterialMaps::from_bytes_with_image_limits(
            &bytes,
            GltfImageLimits {
                max_source_pixels: Some(10),
                max_decoded_bytes: None,
            },
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("paint"), "{error}");
        assert!(error.contains("normal"), "{error}");
        assert!(error.contains("max_source_pixels"), "{error}");
        assert!(!error.contains("occlusion"), "{error}");
    }

    #[test]
    fn a_mirrored_node_flips_tangent_handedness_without_moving_mesh_local_data() {
        let local = vec![Vec4::new(-1.0, 0.0, 0.0, -1.0)];
        let world = Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0));
        let baked = world_tangents(&local, world);
        assert!((baked[0].truncate() - Vec3::X).length() < 1.0e-5);
        assert_eq!(baked[0].w, 1.0);
        assert!(world_tangents(&[], world).is_empty());
    }
}
