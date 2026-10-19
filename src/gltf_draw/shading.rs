//! Tangent-space normals and the directional metallic-roughness response.
//!
//! [`GltfMaterialFrame::submit`](super::GltfMaterialFrame::submit) keeps the
//! existing diffuse preview. These types are used only when a frame is given
//! [`super::GltfModelMaterialMaps`] or a [`GltfShadingModel`].

use glam::{Mat4, Vec3, Vec4};

use super::material::GltfLighting;
use super::{GltfDrawMesh, GltfModel, GltfModelMaterialMaps, GltfPartMaterialMaps};
use crate::models::Vertex;
use crate::quad_gl::ShadingVertex;
use crate::texture::Texture2D;

/// Smallest GGX alpha. Roughness below `0.045` is treated as `0.045` so a
/// perfectly smooth highlight stays finite. This is not a physical roughness.
const MIN_ALPHA: f32 = 0.045 * 0.045;

/// Linear RGB for one shaded fragment, before sRGB output encoding.
///
/// `base_linear` is the sRGB-decoded base color times its factor. `metallic`
/// and `roughness` are the clamped product of the material factors and the
/// metallic-roughness map (green is roughness, blue is metallic). `normal` is
/// the world normal after the tangent frame, or the geometric normal when no
/// normal map is applied. `view_dir` points from the fragment toward the
/// camera. Hemisphere light is diffuse only. The directional light adds a
/// diffuse term and a GGX specular term. There is no environment reflection,
/// occlusion, transmission, or shadow.
///
/// A zero normal uses the hemisphere midpoint and no directional contribution.
///
/// ```
/// use macroquad::gltf_draw::{shade_gltf_surface, GltfLighting};
/// use macroquad::math::Vec3;
///
/// let color = shade_gltf_surface(
///     Vec3::ONE,
///     0.0,
///     1.0,
///     Vec3::Z,
///     Vec3::Z,
///     GltfLighting {
///         sky_color: Vec3::ZERO,
///         ground_color: Vec3::ZERO,
///         directional_direction: Vec3::Z,
///         directional_color: Vec3::ONE,
///         ..GltfLighting::default()
///     },
///     Vec3::ZERO,
/// );
/// assert!((color.x - 0.963183).abs() < 1.0e-4);
/// ```
pub fn shade_gltf_surface(
    base_linear: Vec3,
    metallic: f32,
    roughness: f32,
    normal: Vec3,
    view_dir: Vec3,
    lighting: GltfLighting,
    emissive_linear: Vec3,
) -> Vec3 {
    let metallic = finite_unit(metallic, 0.0);
    let roughness = finite_unit(roughness, 1.0);
    let n = unit(normal).unwrap_or(Vec3::ZERO);
    let v = unit(view_dir).unwrap_or(Vec3::ZERO);
    let l = lighting.directional_direction;
    let ndotl = n.dot(l).max(0.0);
    let sky = (0.5 + 0.5 * n.dot(lighting.hemisphere_up)).clamp(0.0, 1.0);
    let hemi = lighting.ground_color.lerp(lighting.sky_color, sky);
    let c_diff = base_linear * (1.0 - 0.04) * (1.0 - metallic);
    let f0 = Vec3::splat(0.04).lerp(base_linear, metallic);
    let diffuse = c_diff * (hemi + lighting.directional_color * ndotl);
    let specular = directional_specular(n, v, l, ndotl, roughness, f0, lighting.directional_color);
    diffuse + specular + emissive_linear
}

/// World normal from a tangent-space sample.
///
/// `sampled_rgb` is the linear normal-map texel in `0..=1`, not sRGB.
/// `scale` is `normalTexture.scale` and applies to the XY components before
/// the result is normalized. `n`, `t`, and `b` are the world tangent frame
/// from [`gltf_tangent_frame`].
///
/// ```
/// use macroquad::gltf_draw::gltf_tangent_normal;
/// use macroquad::math::Vec3;
///
/// let normal = gltf_tangent_normal(Vec3::Z, Vec3::X, Vec3::Y, Vec3::new(1.0, 0.5, 0.5), 1.0);
/// assert!(normal.abs_diff_eq(Vec3::X, 1.0e-5));
/// ```
pub fn gltf_tangent_normal(n: Vec3, t: Vec3, b: Vec3, sampled_rgb: Vec3, scale: f32) -> Vec3 {
    let mut tangent_space = sampled_rgb * 2.0 - Vec3::ONE;
    tangent_space.x *= scale;
    tangent_space.y *= scale;
    unit(t * tangent_space.x + b * tangent_space.y + n * tangent_space.z).unwrap_or(n)
}

/// World tangent frame for one shaded vertex.
///
/// `geometric_normal` and `tangent` are the loader's node-world values
/// ([`super::GltfPartMaterialMaps`] tangents already include the node's forward
/// vector transform and a mirrored node's handedness flip). `instance` is the
/// additional parent/instance matrix, the same matrix the renderer uploads as
/// `Model`. The normal uses that matrix's inverse transpose. The tangent uses
/// the matrix itself, then Gram-Schmidt against the normal. `handedness` is
/// the stored tangent W. A mirrored instance flips the bitangent.
///
/// Returns [`None`] when the instance is singular or the tangent collapses
/// onto the normal.
///
/// ```
/// use macroquad::gltf_draw::gltf_tangent_frame;
/// use macroquad::math::{Mat4, Vec3};
///
/// let (n, t, b) = gltf_tangent_frame(Vec3::Z, Vec3::X, 1.0, Mat4::from_scale(Vec3::new(-2.0, 1.0, 1.0)))
///     .unwrap();
/// assert!(n.abs_diff_eq(Vec3::Z, 1.0e-5));
/// assert!(t.abs_diff_eq(-Vec3::X, 1.0e-5));
/// assert!(b.abs_diff_eq(Vec3::Y, 1.0e-5));
/// ```
pub fn gltf_tangent_frame(
    geometric_normal: Vec3,
    tangent: Vec3,
    handedness: f32,
    instance: Mat4,
) -> Option<(Vec3, Vec3, Vec3)> {
    if !geometric_normal.is_finite() || !tangent.is_finite() || !handedness.is_finite() {
        return None;
    }
    let n = unit(normal_transform(instance)?.transform_vector3(geometric_normal))?;
    let t = unit(instance.transform_vector3(tangent))?;
    let t = unit(t - n * n.dot(t))?;
    let mirror = if mirrored(instance) { -1.0 } else { 1.0 };
    let b = unit(n.cross(t) * handedness * mirror)?;
    Some((n, t, b))
}

pub(crate) fn normal_transform(matrix: Mat4) -> Option<Mat4> {
    let x = matrix.x_axis.truncate().as_dvec3();
    let y = matrix.y_axis.truncate().as_dvec3();
    let z = matrix.z_axis.truncate().as_dvec3();
    let columns = [y.cross(z), z.cross(x), x.cross(y)];
    let determinant = x.dot(columns[0]);
    if determinant == 0.0 || !determinant.is_finite() {
        return None;
    }
    let magnitude = columns
        .iter()
        .map(|v| v.abs().max_element())
        .fold(0.0, f64::max);
    let scale = determinant.signum() / magnitude;
    Some(Mat4::from_cols(
        (columns[0] * scale).as_vec3().extend(0.0),
        (columns[1] * scale).as_vec3().extend(0.0),
        (columns[2] * scale).as_vec3().extend(0.0),
        Vec4::W,
    ))
}

pub(crate) fn mirrored(matrix: Mat4) -> bool {
    let x = matrix.x_axis.truncate().as_dvec3();
    let y = matrix.y_axis.truncate().as_dvec3();
    let z = matrix.z_axis.truncate().as_dvec3();
    x.dot(y.cross(z)) < 0.0
}

fn finite_unit(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        fallback
    }
}

fn unit(value: Vec3) -> Option<Vec3> {
    let largest = value.abs().max_element();
    if largest <= 0.0 || !value.is_finite() {
        return None;
    }
    let value = (value / largest).normalize();
    value.is_finite().then_some(value)
}

fn directional_specular(
    n: Vec3,
    v: Vec3,
    l: Vec3,
    ndotl: f32,
    roughness: f32,
    f0: Vec3,
    light: Vec3,
) -> Vec3 {
    if ndotl <= 0.0 || n == Vec3::ZERO || v == Vec3::ZERO {
        return Vec3::ZERO;
    }
    let Some(h) = unit(l + v) else {
        return Vec3::ZERO;
    };
    let ndotv = n.dot(v).max(0.0);
    let ndoth = n.dot(h).max(0.0);
    let vdoth = v.dot(h).max(0.0);
    let alpha = (roughness * roughness).max(MIN_ALPHA);
    let a2 = alpha * alpha;
    let denom = ndoth * ndoth * (a2 - 1.0) + 1.0;
    let d = a2 / (std::f32::consts::PI * denom * denom);
    let ggxv = ndotl * (ndotv * ndotv * (1.0 - a2) + a2).sqrt();
    let ggxl = ndotv * (ndotl * ndotl * (1.0 - a2) + a2).sqrt();
    let vis = 0.5 / (ggxv + ggxl);
    let fresnel = f0 + (Vec3::ONE - f0) * (1.0 - vdoth).powi(5);
    fresnel * vis * d * light * ndotl
}

pub(crate) fn validate_shading_maps(
    mesh: &GltfDrawMesh,
    maps: &GltfPartMaterialMaps,
) -> Result<(), String> {
    let count = mesh.mesh.vertices.len();
    if count == 0 || mesh.mesh.indices.is_empty() || !mesh.mesh.indices.len().is_multiple_of(3) {
        return Err(format!(
            "glTF material maps for {} need a complete triangle mesh",
            mesh.path
        ));
    }
    if mesh
        .mesh
        .indices
        .iter()
        .any(|index| *index as usize >= count)
    {
        return Err(format!(
            "glTF material maps for {} reference a missing vertex",
            mesh.path
        ));
    }
    if maps
        .normal
        .as_ref()
        .is_some_and(|map| map.uvs.len() != count)
    {
        return Err(format!(
            "glTF normal-map UVs for {} do not match its {count} vertices",
            mesh.path
        ));
    }
    if maps
        .metallic_roughness
        .as_ref()
        .is_some_and(|map| map.uvs.len() != count)
    {
        return Err(format!(
            "glTF metallic-roughness UVs for {} do not match its {count} vertices",
            mesh.path
        ));
    }
    if maps.normal.is_some() && maps.tangents.len() != count {
        return Err(format!(
            "glTF normal map for {} requires one tangent per vertex",
            mesh.path
        ));
    }
    if !maps.tangents.is_empty() && maps.tangents.len() != count {
        return Err(format!(
            "glTF tangents for {} do not match its {count} vertices",
            mesh.path
        ));
    }
    if let Some(scale) = maps.normal.as_ref().map(|map| map.strength) {
        if !scale.is_finite() {
            return Err(format!(
                "glTF normal-map scale for {} must be finite",
                mesh.path
            ));
        }
    }
    Ok(())
}

pub(crate) fn shading_vertices(
    mesh: &GltfDrawMesh,
    maps: &GltfPartMaterialMaps,
) -> Result<Vec<ShadingVertex>, String> {
    validate_shading_maps(mesh, maps)?;
    Ok(mesh
        .mesh
        .vertices
        .iter()
        .enumerate()
        .map(|(index, vertex)| shading_vertex(vertex, maps, index))
        .collect())
}

fn shading_vertex(vertex: &Vertex, maps: &GltfPartMaterialMaps, index: usize) -> ShadingVertex {
    let tangent = maps.tangents.get(index).copied().unwrap_or(Vec4::ZERO);
    let normal_uv = maps
        .normal
        .as_ref()
        .map(|map| map.uvs[index])
        .unwrap_or(vertex.uv);
    let metallic_uv = maps
        .metallic_roughness
        .as_ref()
        .map(|map| map.uvs[index])
        .unwrap_or(vertex.uv);
    ShadingVertex {
        position: vertex.position,
        uv: vertex.uv,
        normal: vertex.normal.truncate().extend(tangent.w),
        tangent: tangent.truncate().extend(0.0),
        normal_uv,
        metallic_uv,
    }
}

/// GPU shading buffers parallel to a retained model's chunks.
///
/// Build this while the CPU mesh still exists. Retained geometry buffers stay
/// unchanged; these buffers hold the tangent frame and map UVs the lit shader
/// reads. Dropping the CPU model afterwards does not drop the map textures
/// cloned into this value.
///
/// ```no_run
/// use macroquad::prelude::*;
/// # fn upload(model: &GltfModel, maps: &GltfModelMaterialMaps) -> Result<(), String> {
/// let shading = GltfShadingModel::from_model(model, maps)?;
/// assert_eq!(shading.parts().len(), model.parts.len());
/// # Ok(()) }
/// ```
#[derive(Clone)]
pub struct GltfShadingModel {
    parts: Vec<GltfShadingPart>,
}

/// One retained chunk's shading buffers and map textures.
///
/// Pair it with the [`super::RetainedGltfDrawMesh`] at the same index in
/// [`GltfShadingModel::parts`] and [`super::RetainedGltfModel::parts`].
/// The fields stay private; pass the whole [`GltfShadingModel`] to a shaded
/// retained submission.
///
/// ```no_run
/// use macroquad::prelude::*;
/// # fn pair(shading: &GltfShadingModel, model: &RetainedGltfModel) {
/// assert_eq!(shading.parts().len(), model.parts.len());
/// # }
/// ```
#[derive(Clone)]
pub struct GltfShadingPart {
    pub(crate) buffers: std::sync::Arc<crate::quad_gl::ShadingBuffers>,
    pub(crate) normal_texture: Option<Texture2D>,
    pub(crate) normal_scale: f32,
    pub(crate) metallic_texture: Option<Texture2D>,
}

impl GltfShadingModel {
    /// Uploads one shading mesh per chunk.
    ///
    /// `maps.parts` must match `model.parts` in order. A normal map without
    /// one tangent per vertex is an error, and nothing is uploaded in that
    /// case. An active graphics context is required when the model is not empty.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn upload(model: GltfModel, maps: GltfModelMaterialMaps) -> Result<(), String> {
    /// let shading = GltfShadingModel::from_model(&model, &maps)?;
    /// let retained = RetainedGltfModel::from_model(model, false).map_err(|e| e.to_string())?;
    /// assert_eq!(shading.parts().len(), retained.parts.len());
    /// # Ok(()) }
    /// ```
    pub fn from_model(model: &GltfModel, maps: &GltfModelMaterialMaps) -> Result<Self, String> {
        if maps.parts.len() != model.parts.len() {
            return Err(format!(
                "glTF material maps have {} chunks and the model has {}",
                maps.parts.len(),
                model.parts.len()
            ));
        }
        let built = model
            .parts
            .iter()
            .zip(&maps.parts)
            .map(|(part, maps)| {
                let vertices = shading_vertices(part, maps)?;
                Ok((vertices, part.mesh.indices.clone(), maps))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut parts = Vec::with_capacity(built.len());
        for (vertices, indices, maps) in built {
            let context = crate::get_context();
            let buffers =
                context
                    .gl
                    .upload_shading_mesh(&mut *context.quad_context, &vertices, &indices);
            parts.push(GltfShadingPart {
                buffers,
                normal_texture: maps.normal.as_ref().map(|map| map.texture.clone()),
                normal_scale: maps.normal.as_ref().map(|map| map.strength).unwrap_or(1.0),
                metallic_texture: maps
                    .metallic_roughness
                    .as_ref()
                    .map(|map| map.texture.clone()),
            });
        }
        Ok(Self { parts })
    }

    /// Shading chunks in [`GltfModel::parts`](super::GltfModel::parts) order.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn inspect(shading: &GltfShadingModel) {
    /// assert!(shading.parts().is_empty() || !shading.parts().is_empty());
    /// # }
    /// ```
    pub fn parts(&self) -> &[GltfShadingPart] {
        &self.parts
    }
}

pub(crate) const SHADED_VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 normal;
attribute vec4 tangent;
attribute vec2 normal_uv;
attribute vec2 metallic_uv;
uniform mat4 Model;
uniform mat4 Projection;
uniform mat4 NormalTransform;
uniform mediump vec4 LightParams;
varying mediump vec2 uv;
varying mediump vec2 v_normal_uv;
varying mediump vec2 v_metallic_uv;
varying mediump vec3 v_normal;
varying mediump vec3 v_tangent;
varying mediump vec3 v_bitangent;
varying mediump vec3 v_world;
void main() {
    vec4 world = Model * vec4(position, 1.0);
    gl_Position = Projection * world;
    uv = texcoord;
    v_normal_uv = normal_uv;
    v_metallic_uv = metallic_uv;
    v_world = world.xyz;
    highp vec3 n = (NormalTransform * vec4(normal.xyz, 0.0)).xyz;
    highp float n_scale = max(max(abs(n.x), abs(n.y)), abs(n.z));
    n = n_scale > 0.0 ? normalize(n / n_scale) : vec3(0.0);
    highp vec3 t = (Model * vec4(tangent.xyz, 0.0)).xyz;
    highp float t_scale = max(max(abs(t.x), abs(t.y)), abs(t.z));
    t = t_scale > 0.0 ? normalize(t / t_scale) : vec3(0.0);
    t = t - n * dot(n, t);
    highp float g_scale = max(max(abs(t.x), abs(t.y)), abs(t.z));
    t = g_scale > 0.0 ? normalize(t / g_scale) : vec3(0.0);
    highp vec3 b = cross(n, t) * normal.w * LightParams.z;
    v_normal = n;
    v_tangent = t;
    v_bitangent = b;
}
"#;

pub(crate) const SHADED_FRAGMENT: &str = r#"#version 100
precision mediump float;
varying mediump vec2 uv;
varying mediump vec2 v_normal_uv;
varying mediump vec2 v_metallic_uv;
varying mediump vec3 v_normal;
varying mediump vec3 v_tangent;
varying mediump vec3 v_bitangent;
varying mediump vec3 v_world;
uniform sampler2D Texture;
uniform sampler2D EmissiveTexture;
uniform sampler2D NormalTexture;
uniform sampler2D MetallicRoughnessTexture;
uniform vec4 BaseColor;
uniform vec4 Emission;
uniform vec4 AlphaOutput;
uniform vec4 LightParams;
uniform vec4 HemisphereUp;
uniform vec4 SkyColor;
uniform vec4 GroundColor;
uniform vec4 LightDirection;
uniform vec4 LightColor;
uniform vec4 MapParams;
uniform vec4 CameraPos;
const float PI = 3.14159265;
vec3 lighting_normal(vec3 value) {
    float largest = max(max(abs(value.x), abs(value.y)), abs(value.z));
    if (!(largest > 0.0)) return vec3(0.0);
    return normalize(value / largest);
}
vec3 decode_srgb(vec3 rgb) {
    vec3 low = rgb / 12.92;
    vec3 high = pow((rgb + 0.055) / 1.055, vec3(2.4));
    return mix(low, high, step(vec3(0.04045), rgb));
}
vec3 encode_srgb(vec3 rgb) {
    rgb = max(rgb, vec3(0.0));
    vec3 low = rgb * 12.92;
    vec3 high = 1.055 * pow(rgb, vec3(1.0 / 2.4)) - 0.055;
    return mix(low, high, step(vec3(0.0031308), rgb));
}
vec3 surface_normal() {
    vec3 n = lighting_normal(v_normal);
    vec3 t = lighting_normal(v_tangent);
    vec3 b = lighting_normal(v_bitangent);
    float side = 1.0;
    if (LightParams.y > 0.5) side = (gl_FrontFacing ? 1.0 : -1.0) * LightParams.z;
    // Flip the whole face. N and T both take `side`, so cross(N, T) is unchanged.
    // B already includes the mirror sign from the vertex stage and must not
    // take `side` again.
    n *= side;
    t *= side;
    if (MapParams.x > 0.5 && dot(t, t) > 0.5) {
        vec3 ts = texture2D(NormalTexture, v_normal_uv).xyz * 2.0 - 1.0;
        ts.xy *= MapParams.y;
        n = lighting_normal(t * ts.x + b * ts.y + n * ts.z);
    }
    return n;
}
void main() {
    vec4 base = texture2D(Texture, uv);
    float alpha = clamp(base.a * BaseColor.a, 0.0, 1.0);
    if (AlphaOutput.y > 0.5 && AlphaOutput.y < 1.5 && alpha < AlphaOutput.x) discard;
    vec4 mr = texture2D(MetallicRoughnessTexture, v_metallic_uv);
    float metallic = clamp(MapParams.z * mr.b, 0.0, 1.0);
    float roughness = clamp(MapParams.w * mr.g, 0.0, 1.0);
    vec3 n = surface_normal();
    vec3 view = lighting_normal(CameraPos.xyz - v_world);
    vec3 l = LightDirection.xyz;
    float ndotl = max(dot(n, l), 0.0);
    float sky = clamp(0.5 + 0.5 * dot(n, HemisphereUp.xyz), 0.0, 1.0);
    vec3 hemi = mix(GroundColor.rgb, SkyColor.rgb, sky);
    vec3 base_linear = decode_srgb(base.rgb) * BaseColor.rgb;
    vec3 c_diff = base_linear * (1.0 - 0.04) * (1.0 - metallic);
    vec3 f0 = mix(vec3(0.04), base_linear, metallic);
    vec3 diffuse = c_diff * (hemi + LightColor.rgb * ndotl);
    vec3 specular = vec3(0.0);
    if (ndotl > 0.0 && dot(n, n) > 0.5 && dot(view, view) > 0.5) {
        vec3 h = lighting_normal(l + view);
        float ndotv = max(dot(n, view), 0.0);
        float ndoth = max(dot(n, h), 0.0);
        float vdoth = max(dot(view, h), 0.0);
        float alpha_rough = max(roughness * roughness, 0.002025);
        float a2 = alpha_rough * alpha_rough;
        float denom = ndoth * ndoth * (a2 - 1.0) + 1.0;
        float d = a2 / (PI * denom * denom);
        float ggxv = ndotl * sqrt(ndotv * ndotv * (1.0 - a2) + a2);
        float ggxl = ndotv * sqrt(ndotl * ndotl * (1.0 - a2) + a2);
        float vis = 0.5 / (ggxv + ggxl);
        vec3 fresnel = f0 + (vec3(1.0) - f0) * pow(1.0 - vdoth, 5.0);
        specular = fresnel * vis * d * LightColor.rgb * ndotl;
    }
    vec3 rgb = diffuse + specular + decode_srgb(texture2D(EmissiveTexture, uv).rgb) * Emission.rgb;
    if (AlphaOutput.z > 0.5) rgb = encode_srgb(rgb);
    if (AlphaOutput.y < 1.5) alpha = 1.0;
    gl_FragColor = vec4(rgb, alpha);
}
"#;

pub(crate) const SHADED_METAL: &str = r#"#include <metal_stdlib>
using namespace metal;
struct Vertex {
    float3 position [[attribute(0)]];
    float2 texcoord [[attribute(1)]];
    float4 normal [[attribute(2)]];
    float4 tangent [[attribute(3)]];
    float2 normal_uv [[attribute(4)]];
    float2 metallic_uv [[attribute(5)]];
};
struct RasterizerData {
    float4 position [[position]];
    float2 uv [[user(locn0)]];
    float2 normal_uv [[user(locn1)]];
    float2 metallic_uv [[user(locn2)]];
    float3 normal [[user(locn3)]];
    float3 tangent [[user(locn4)]];
    float3 bitangent [[user(locn5)]];
    float3 world [[user(locn6)]];
};
struct Uniforms {
    float4x4 Projection;
    float4x4 Model;
    float4 _Time;
    float4 BaseColor;
    float4 Emission;
    float4 AlphaOutput;
    float4x4 NormalTransform;
    float4 LightParams;
    float4 HemisphereUp;
    float4 SkyColor;
    float4 GroundColor;
    float4 LightDirection;
    float4 LightColor;
    float4 MapParams;
    float4 CameraPos;
};
vertex RasterizerData vertexShader(Vertex v [[stage_in]], constant Uniforms& u [[buffer(0)]]) {
    RasterizerData out;
    float4 world = u.Model * float4(v.position, 1.0);
    out.position = u.Projection * world;
    out.uv = v.texcoord;
    out.normal_uv = v.normal_uv;
    out.metallic_uv = v.metallic_uv;
    out.world = world.xyz;
    float3 n = (u.NormalTransform * float4(v.normal.xyz, 0.0)).xyz;
    float n_scale = max(max(abs(n.x), abs(n.y)), abs(n.z));
    n = n_scale > 0.0 ? normalize(n / n_scale) : float3(0.0);
    float3 t = (u.Model * float4(v.tangent.xyz, 0.0)).xyz;
    float t_scale = max(max(abs(t.x), abs(t.y)), abs(t.z));
    t = t_scale > 0.0 ? normalize(t / t_scale) : float3(0.0);
    t = t - n * dot(n, t);
    float g_scale = max(max(abs(t.x), abs(t.y)), abs(t.z));
    t = g_scale > 0.0 ? normalize(t / g_scale) : float3(0.0);
    out.normal = n;
    out.tangent = t;
    out.bitangent = cross(n, t) * v.normal.w * u.LightParams.z;
    return out;
}
float3 decode_srgb(float3 rgb) {
    return select(pow((rgb + 0.055) / 1.055, float3(2.4)), rgb / 12.92, rgb <= 0.04045);
}
float3 encode_srgb(float3 rgb) {
    rgb = max(rgb, float3(0.0));
    return select(1.055 * pow(rgb, float3(1.0 / 2.4)) - 0.055, rgb * 12.92, rgb <= 0.0031308);
}
float3 lighting_normal(float3 value) {
    float largest = max(max(abs(value.x), abs(value.y)), abs(value.z));
    if (!(largest > 0.0)) return float3(0.0);
    return normalize(value / largest);
}
fragment float4 fragmentShader(RasterizerData in [[stage_in]],
    bool front [[front_facing]],
    constant Uniforms& u [[buffer(0)]],
    texture2d<float> Texture [[texture(0)]], sampler TextureSampler [[sampler(0)]],
    texture2d<float> EmissiveTexture [[texture(2)]], sampler EmissiveSampler [[sampler(2)]],
    texture2d<float> NormalTexture [[texture(3)]], sampler NormalSampler [[sampler(3)]],
    texture2d<float> MetallicRoughnessTexture [[texture(4)]], sampler MetallicSampler [[sampler(4)]]) {
    float4 base = Texture.sample(TextureSampler, in.uv);
    float alpha = clamp(base.a * u.BaseColor.a, 0.0, 1.0);
    if (u.AlphaOutput.y > 0.5 && u.AlphaOutput.y < 1.5 && alpha < u.AlphaOutput.x) discard_fragment();
    float4 mr = MetallicRoughnessTexture.sample(MetallicSampler, in.metallic_uv);
    float metallic = clamp(u.MapParams.z * mr.b, 0.0, 1.0);
    float roughness = clamp(u.MapParams.w * mr.g, 0.0, 1.0);
    float3 n = lighting_normal(in.normal);
    float3 t = lighting_normal(in.tangent);
    float3 b = lighting_normal(in.bitangent);
    // miniquad 0.4.11 leaves Metal front winding clockwise, so `front` is the
    // opposite of the logical CCW front used by the GL double-sided path.
    float side = 1.0;
    if (u.LightParams.y > 0.5) side = (front ? -1.0 : 1.0) * u.LightParams.z;
    // B already includes the mirror sign. Flipping N and T together keeps it.
    n *= side;
    t *= side;
    if (u.MapParams.x > 0.5 && dot(t, t) > 0.5) {
        float3 ts = NormalTexture.sample(NormalSampler, in.normal_uv).xyz * 2.0 - 1.0;
        ts.xy *= u.MapParams.y;
        n = lighting_normal(t * ts.x + b * ts.y + n * ts.z);
    }
    float3 view = lighting_normal(u.CameraPos.xyz - in.world);
    float3 l = u.LightDirection.xyz;
    float ndotl = max(dot(n, l), 0.0);
    float sky = clamp(0.5 + 0.5 * dot(n, u.HemisphereUp.xyz), 0.0, 1.0);
    float3 hemi = mix(u.GroundColor.rgb, u.SkyColor.rgb, sky);
    float3 base_linear = decode_srgb(base.rgb) * u.BaseColor.rgb;
    float3 c_diff = base_linear * (1.0 - 0.04) * (1.0 - metallic);
    float3 f0 = mix(float3(0.04), base_linear, metallic);
    float3 diffuse = c_diff * (hemi + u.LightColor.rgb * ndotl);
    float3 specular = float3(0.0);
    if (ndotl > 0.0 && dot(n, n) > 0.5 && dot(view, view) > 0.5) {
        float3 h = lighting_normal(l + view);
        float ndotv = max(dot(n, view), 0.0);
        float ndoth = max(dot(n, h), 0.0);
        float vdoth = max(dot(view, h), 0.0);
        float alpha_rough = max(roughness * roughness, 0.002025);
        float a2 = alpha_rough * alpha_rough;
        float denom = ndoth * ndoth * (a2 - 1.0) + 1.0;
        float d = a2 / (3.14159265 * denom * denom);
        float ggxv = ndotl * sqrt(ndotv * ndotv * (1.0 - a2) + a2);
        float ggxl = ndotv * sqrt(ndotl * ndotl * (1.0 - a2) + a2);
        float vis = 0.5 / (ggxv + ggxl);
        float3 fresnel = f0 + (float3(1.0) - f0) * pow(1.0 - vdoth, 5.0);
        specular = fresnel * vis * d * u.LightColor.rgb * ndotl;
    }
    float3 rgb = diffuse + specular + decode_srgb(EmissiveTexture.sample(EmissiveSampler, in.uv).rgb) * u.Emission.rgb;
    if (u.AlphaOutput.z > 0.5) rgb = encode_srgb(rgb);
    if (u.AlphaOutput.y < 1.5) alpha = 1.0;
    return float4(rgb, alpha);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn white_light(direction: Vec3) -> GltfLighting {
        GltfLighting {
            sky_color: Vec3::ZERO,
            ground_color: Vec3::ZERO,
            directional_direction: direction,
            directional_color: Vec3::ONE,
            ..GltfLighting::default()
        }
    }

    #[test]
    fn rough_dielectric_matches_the_punctual_split() {
        let color = shade_gltf_surface(
            Vec3::ONE,
            0.0,
            1.0,
            Vec3::Z,
            Vec3::Z,
            white_light(Vec3::Z),
            Vec3::ZERO,
        );
        assert!((color.x - 0.963183).abs() < 1.0e-4, "{color:?}");
        assert!(color.abs_diff_eq(Vec3::splat(color.x), 1.0e-5));
    }

    #[test]
    fn rough_metal_keeps_specular_and_drops_diffuse() {
        let color = shade_gltf_surface(
            Vec3::X,
            1.0,
            1.0,
            Vec3::Z,
            Vec3::Z,
            white_light(Vec3::Z),
            Vec3::ZERO,
        );
        assert!((color.x - 0.079577).abs() < 1.0e-4, "{color:?}");
        assert_eq!(color.y, 0.0);
        assert_eq!(color.z, 0.0);
    }

    #[test]
    fn mirror_and_nonuniform_instance_rebuild_the_tangent_frame() {
        let (n, t, b) = gltf_tangent_frame(
            Vec3::Z,
            Vec3::X,
            1.0,
            Mat4::from_scale(Vec3::new(-2.0, 1.0, 1.0)),
        )
        .unwrap();
        assert!(n.abs_diff_eq(Vec3::Z, 1.0e-5));
        assert!(t.abs_diff_eq(-Vec3::X, 1.0e-5));
        assert!(b.abs_diff_eq(Vec3::Y, 1.0e-5));

        let geometric = Vec3::new(0.0, 1.0, 1.0).normalize();
        let (n, t, b) = gltf_tangent_frame(
            geometric,
            Vec3::X,
            1.0,
            Mat4::from_scale(Vec3::new(2.0, 1.0, 0.5)),
        )
        .unwrap();
        let expected_n = Vec3::new(0.0, 0.4472136, 0.8944272);
        assert!(n.abs_diff_eq(expected_n, 1.0e-4), "{n:?}");
        assert!(t.abs_diff_eq(Vec3::X, 1.0e-5));
        assert!(b.abs_diff_eq(Vec3::new(0.0, 0.8944272, -0.4472136), 1.0e-4));

        let shear = Mat4::from_cols(Vec4::new(1.0, 0.5, 0.0, 0.0), Vec4::Y, Vec4::Z, Vec4::W);
        let (n, t, _) = gltf_tangent_frame(Vec3::Z, Vec3::X, 1.0, shear).unwrap();
        assert!(n.abs_diff_eq(Vec3::Z, 1.0e-5));
        assert!(t.abs_diff_eq(Vec3::new(1.0, 0.5, 0.0).normalize(), 1.0e-5));
    }

    #[test]
    fn normal_map_scale_and_flat_sample_follow_the_frame() {
        let flat = gltf_tangent_normal(Vec3::Z, Vec3::X, Vec3::Y, Vec3::new(0.5, 0.5, 1.0), 2.0);
        assert!(flat.abs_diff_eq(Vec3::Z, 1.0e-5));
        let tilted = gltf_tangent_normal(Vec3::Z, Vec3::X, Vec3::Y, Vec3::new(1.0, 0.5, 0.5), -1.0);
        assert!(tilted.abs_diff_eq(-Vec3::X, 1.0e-5));
    }
}
