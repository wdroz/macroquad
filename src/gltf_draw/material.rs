use glam::{Mat4, Vec3, Vec4};
use miniquad::{
    Backend, BlendFactor, BlendState, BlendValue, Comparison, CullFace, Equation, FrontFaceOrder,
    PipelineParams, ShaderSource, UniformDesc, UniformType,
};

use super::{
    GltfAlphaMode, GltfDrawMesh, GltfModel, GltfSurface, RetainedGltfDrawMesh, RetainedGltfModel,
};
use crate::{
    material::{gl_use_default_material, gl_use_material, load_material, Material, MaterialParams},
    models::draw_mesh,
    texture::Texture2D,
    Error,
};

/// Color values written by the opt-in glTF material renderer.
///
/// Both modes decode base-color and emissive textures from sRGB and apply
/// their factors in linear space. Alpha is always linear. This selects the
/// final RGB conversion, not a framebuffer format or a tone mapper.
///
/// ```
/// use macroquad::prelude::*;
/// assert_eq!(GltfColorOutput::default(), GltfColorOutput::Srgb);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GltfColorOutput {
    /// Encode the fragment's RGB to sRGB for an ordinary display framebuffer.
    /// Blending occurs afterwards in the destination's encoded space. This is
    /// convenient for direct rendering, but is not linear-light compositing.
    #[default]
    Srgb,
    /// Write linear RGB. For linear-light transparency, draw the whole scene
    /// into a linear target, then encode once in a final fullscreen pass.
    /// The caller owns that target and resolve. Ordinary RGBA8 targets clamp
    /// emission above one; HDR and tone mapping are not provided here.
    Linear,
}

/// Submitted chunk counts for one material frame, not GPU timing or pixels.
///
/// ```
/// use macroquad::prelude::*;
/// let counts = GltfMaterialStats::default();
/// assert_eq!(counts.blend_draws, 0);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GltfMaterialStats {
    /// Opaque chunks submitted with depth testing and depth writes.
    pub opaque_draws: usize,
    /// Alpha-mask chunks submitted with depth testing and depth writes.
    pub mask_draws: usize,
    /// Blended chunks submitted with depth testing and no depth writes.
    pub blend_draws: usize,
    /// Chunks skipped because the composed matrix, center or depth is invalid.
    pub skipped_draws: usize,
}

/// Opt-in base-color/emissive preview renderer for CPU and retained glTF models.
///
/// Construct inside a running macroquad application. Set a camera with a depth
/// buffer, begin one collection, submit **all** model instances, then draw it.
/// Opaque and mask chunks precede a globally sorted blend pass. Only chunk
/// centers are sorted: intersecting triangles, overlapping bounds and concave
/// transparent meshes can still composite incorrectly. Split geometry or use
/// an application renderer when that limitation matters.
///
/// Base-color and emissive image RGB are decoded from sRGB. Exact linear
/// [`GltfSurface`] factors are applied once; the loader's byte-quantized vertex
/// colors are deliberately ignored, including application edits to them.
/// Existing RGBA8 texture filtering and mipmap generation operate on encoded
/// values before the shader decodes the sample; this is not hardware sRGB
/// texture filtering. Neither output mode changes that upload-format limit.
/// An absent texture means white. Non-unlit materials preview base plus
/// emission without lighting; `KHR_materials_unlit` uses only base color.
/// Metallic, roughness, normals, occlusion, transmission, refraction and
/// shadows are not evaluated. Inspect [`super::gltf_material_warnings`] before
/// discarding the input bytes to identify unsupported source features.
///
/// [`GltfMaterialFrame::draw`] flushes pending render commands before returning,
/// making it safe to drop the renderer or models immediately afterwards. It
/// leaves the default material selected and clears the base texture binding;
/// select a custom material again for subsequent custom drawing. Camera,
/// render target, viewport, scissor and the model stack stay unchanged. A
/// dropped frame that was never drawn has no rendering side effects.
///
/// Existing [`GltfModel::draw`] and [`RetainedGltfModel::draw`] are unchanged.
/// OpenGL/WebGL and Metal shaders are supplied; this is not a PBR renderer.
/// The renderer and its borrowed frames cannot be sent to other threads.
///
/// ```no_run
/// use macroquad::prelude::*;
/// # fn example(model: &GltfModel, camera: &Camera3D) -> Result<(), String> {
/// let mut renderer = GltfMaterialRenderer::new().map_err(|e| e.to_string())?;
/// set_camera(camera);
/// let mut frame = renderer.begin(camera.position, camera.target - camera.position)?;
/// frame.submit(model, Mat4::IDENTITY)?;
/// frame.submit(model, Mat4::from_translation(Vec3::X))?;
/// let counts = frame.draw();
/// # Ok(()) }
/// ```
pub struct GltfMaterialRenderer {
    // Opaque/mask and blend, each with double-sided, CCW and CW variants.
    materials: Vec<Material>,
    white: Texture2D,
    _not_send_sync: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl GltfMaterialRenderer {
    /// Creates six shader pipelines inside the active graphics context.
    ///
    /// Keep this renderer across frames. Shader compilation errors are
    /// returned; successfully created partial pipelines are released on error.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// let renderer = GltfMaterialRenderer::new().expect("material shaders");
    /// ```
    pub fn new() -> Result<Self, Error> {
        let backend = crate::get_context().quad_context.info().backend;
        let mut materials = Vec::with_capacity(6);
        for blend in [false, true] {
            for sidedness in 0..3 {
                materials.push(load_material(
                    match backend {
                        Backend::OpenGl => ShaderSource::Glsl {
                            vertex: VERTEX,
                            fragment: FRAGMENT,
                        },
                        Backend::Metal => ShaderSource::Msl { program: METAL },
                    },
                    MaterialParams {
                        pipeline_params: pipeline_params(blend, sidedness),
                        uniforms: vec![
                            UniformDesc::new("BaseColor", UniformType::Float4),
                            UniformDesc::new("Emission", UniformType::Float4),
                            UniformDesc::new("AlphaOutput", UniformType::Float4),
                        ],
                        textures: vec!["EmissiveTexture".into()],
                    },
                )?);
            }
        }
        Ok(Self {
            materials,
            white: Texture2D::unmanaged(crate::get_context().gl.white_texture),
            _not_send_sync: std::marker::PhantomData,
        })
    }

    /// Starts one collection with world-space camera position and look direction.
    ///
    /// The direction's magnitude is ignored. Non-finite position/direction or
    /// a zero direction returns an error without changing render state. These
    /// values only control sorting; call [`crate::camera::set_camera`] yourself.
    /// The active camera and low-level parent model matrix at `draw()` apply to
    /// every submitted item. Keep one frame for all transparent model instances.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn example(renderer: &mut GltfMaterialRenderer) -> Result<(), String> {
    /// let frame = renderer.begin(vec3(0., 0., 5.), -Vec3::Z)?;
    /// frame.draw();
    /// # Ok(()) }
    /// ```
    pub fn begin(
        &mut self,
        camera_position: Vec3,
        camera_forward: Vec3,
    ) -> Result<GltfMaterialFrame<'_>, String> {
        let forward = checked_direction(camera_position, camera_forward)?;
        Ok(GltfMaterialFrame {
            renderer: self,
            camera_position,
            camera_forward: forward,
            output: GltfColorOutput::Srgb,
            draws: Vec::new(),
        })
    }
}

/// Borrowed collection of instances for a single material pass sequence.
///
/// Models and the renderer stay borrowed until the collection is drawn or
/// dropped, so their resources cannot disappear during submission. Submission
/// performs no uploads or rendering. Call [`Self::draw`] to finish.
///
/// ```no_run
/// use macroquad::prelude::*;
/// # fn example(renderer: &mut GltfMaterialRenderer, model: &RetainedGltfModel) -> Result<(), String> {
/// let mut frame = renderer.begin(Vec3::Z * 5., -Vec3::Z)?;
/// frame.submit_retained(model, Mat4::IDENTITY)?;
/// frame.draw();
/// # Ok(()) }
/// ```
pub struct GltfMaterialFrame<'a> {
    renderer: &'a mut GltfMaterialRenderer,
    camera_position: Vec3,
    camera_forward: Vec3,
    output: GltfColorOutput,
    draws: Vec<Draw<'a>>,
}

impl<'a> GltfMaterialFrame<'a> {
    /// Selects the final RGB conversion for every item in this collection.
    ///
    /// Defaults to [`GltfColorOutput::Srgb`]. This does not allocate a render
    /// target or enable framebuffer sRGB. See [`GltfColorOutput`] for blending.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn example(frame: &mut GltfMaterialFrame<'_>) {
    /// frame.set_output(GltfColorOutput::Linear);
    /// # }
    /// ```
    pub fn set_output(&mut self, output: GltfColorOutput) {
        self.output = output;
    }

    /// Collects all CPU chunks with an additional affine instance transform.
    ///
    /// Non-finite or projective matrices return an error and append nothing.
    /// Chunk centers/bounds must describe any application-edited geometry.
    /// CPU meshes still obey the ordinary streaming draw-call capacity limits.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn example<'a>(frame: &mut GltfMaterialFrame<'a>, model: &'a GltfModel) -> Result<(), String> {
    /// frame.submit(model, Mat4::from_translation(Vec3::X))?;
    /// # Ok(()) }
    /// ```
    pub fn submit(&mut self, model: &'a GltfModel, transform: Mat4) -> Result<(), String> {
        check_transform(transform)?;
        self.draws.extend(model.parts.iter().map(|part| Draw {
            part: Part::Cpu(part),
            transform,
            depth: 0.0,
        }));
        Ok(())
    }

    /// Collects all retained chunks without uploading or copying geometry.
    ///
    /// The same transform validation and global transparency order apply as
    /// for [`Self::submit`]. CPU and retained instances may share a collection.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn example<'a>(frame: &mut GltfMaterialFrame<'a>, model: &'a RetainedGltfModel) -> Result<(), String> {
    /// frame.submit_retained(model, Mat4::IDENTITY)?;
    /// # Ok(()) }
    /// ```
    pub fn submit_retained(
        &mut self,
        model: &'a RetainedGltfModel,
        transform: Mat4,
    ) -> Result<(), String> {
        check_transform(transform)?;
        self.draws.extend(model.parts.iter().map(|part| Draw {
            part: Part::Retained(part),
            transform,
            depth: 0.0,
        }));
        Ok(())
    }

    /// Draws opaque/masked chunks, then all blended chunks back to front.
    ///
    /// Sorting uses `(transformed_center - camera_position).dot(forward)`;
    /// equal-depth chunks retain submission order. Opaque/mask writes depth,
    /// blend does not; all use `LessOrEqual` depth testing. Mask keeps alpha
    /// equal to its cutoff and writes opaque alpha. Blend uses straight-alpha
    /// source-over RGB and alpha. Single-sided chunks cull back faces, flipping
    /// front-face order for a reflected composed transform. Singular transforms
    /// have no well-defined handedness and use the ordinary front-face order.
    ///
    /// Flushes queued drawing (including earlier commands) before returning,
    /// without committing the frame or collecting retired resources. Restores
    /// the default material, and preserves the camera and model stack. The
    /// renderer retains no model textures after this call. Invalid composed
    /// affine transforms or non-finite chunk centers/depths are skipped.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn example(frame: GltfMaterialFrame<'_>) {
    /// let counts = frame.draw();
    /// println!("{} transparent chunks", counts.blend_draws);
    /// # }
    /// ```
    pub fn draw(mut self) -> GltfMaterialStats {
        let parent = crate::get_context().gl.model_matrix();
        let mut stats = GltfMaterialStats::default();
        self.draws.retain_mut(|draw| {
            let composed = parent * draw.transform;
            let center = composed.transform_point3(draw.part.center());
            let depth = (center - self.camera_position).dot(self.camera_forward);
            if !is_affine(composed) || !center.is_finite() || !depth.is_finite() {
                stats.skipped_draws += 1;
                return false;
            }
            draw.depth = depth;
            true
        });
        self.draws.sort_by(|a, b| {
            let a_blend = a.part.surface().alpha_mode == GltfAlphaMode::Blend;
            let b_blend = b.part.surface().alpha_mode == GltfAlphaMode::Blend;
            a_blend.cmp(&b_blend).then_with(|| {
                if a_blend {
                    b.depth.total_cmp(&a.depth)
                } else {
                    std::cmp::Ordering::Equal
                }
            })
        });
        for draw in &self.draws {
            let surface = draw.part.surface();
            let blend = surface.alpha_mode == GltfAlphaMode::Blend;
            let side = if surface.double_sided {
                0
            } else if mirrored(parent * draw.transform) {
                2
            } else {
                1
            };
            let material = &self.renderer.materials[usize::from(blend) * 3 + side];
            gl_use_material(material);
            material.set_uniform("BaseColor", Vec4::from_array(surface.base_color));
            material.set_uniform(
                "Emission",
                if surface.unlit {
                    Vec4::ZERO
                } else {
                    Vec3::from_array(surface.emissive).extend(0.0)
                },
            );
            let mode = match surface.alpha_mode {
                GltfAlphaMode::Opaque => {
                    stats.opaque_draws += 1;
                    0.0
                }
                GltfAlphaMode::Mask => {
                    stats.mask_draws += 1;
                    1.0
                }
                GltfAlphaMode::Blend => {
                    stats.blend_draws += 1;
                    2.0
                }
            };
            material.set_uniform(
                "AlphaOutput",
                Vec4::new(
                    surface.alpha_cutoff,
                    mode,
                    if self.output == GltfColorOutput::Srgb {
                        1.0
                    } else {
                        0.0
                    },
                    0.0,
                ),
            );
            material.set_texture(
                "EmissiveTexture",
                draw.part.emissive().unwrap_or(&self.renderer.white).clone(),
            );
            match draw.part {
                Part::Cpu(part) => {
                    crate::get_context().gl.push_model_matrix(draw.transform);
                    draw_mesh(&part.mesh);
                    crate::get_context().gl.pop_model_matrix();
                }
                Part::Retained(part) => part.draw(draw.transform),
            }
        }
        // Material pipelines are not owned by queued draws. Complete them while
        // this frame still borrows the renderer, before allowing it to drop.
        crate::get_context().perform_render_passes();
        gl_use_default_material();
        crate::get_context().gl.texture(None);
        // Custom bindings otherwise keep the previous scene's emissive image
        // alive indefinitely, even after that model has been dropped.
        for material in &self.renderer.materials {
            material.set_texture("EmissiveTexture", self.renderer.white.clone());
        }
        stats
    }
}

enum Part<'a> {
    Cpu(&'a GltfDrawMesh),
    Retained(&'a RetainedGltfDrawMesh),
}

impl Part<'_> {
    fn surface(&self) -> &GltfSurface {
        match self {
            Self::Cpu(part) => &part.surface,
            Self::Retained(part) => &part.surface,
        }
    }
    fn center(&self) -> Vec3 {
        match self {
            Self::Cpu(part) => part.center,
            Self::Retained(part) => part.center,
        }
    }
    fn emissive(&self) -> Option<&Texture2D> {
        match self {
            Self::Cpu(part) => part.emissive_texture.as_ref(),
            Self::Retained(part) => part.emissive_texture.as_ref(),
        }
    }
}

struct Draw<'a> {
    part: Part<'a>,
    transform: Mat4,
    depth: f32,
}

fn is_affine(matrix: Mat4) -> bool {
    matrix.is_finite() && matrix.row(3) == Vec4::W
}

fn check_transform(matrix: Mat4) -> Result<(), String> {
    if is_affine(matrix) {
        Ok(())
    } else {
        Err("glTF material instance transform must be finite and affine".into())
    }
}

fn checked_direction(position: Vec3, direction: Vec3) -> Result<Vec3, String> {
    let largest = direction.abs().max_element();
    if !position.is_finite() || !direction.is_finite() || largest == 0.0 {
        return Err(
            "glTF material camera position/direction must be finite and direction nonzero".into(),
        );
    }
    // Scaling first avoids overflow/underflow for valid, very long/short input.
    Ok((direction / largest).normalize())
}

fn mirrored(matrix: Mat4) -> bool {
    // f64 avoids overflow in the determinant for finite f32 affine matrices.
    let x = matrix.x_axis.truncate().as_dvec3();
    let y = matrix.y_axis.truncate().as_dvec3();
    let z = matrix.z_axis.truncate().as_dvec3();
    x.dot(y.cross(z)) < 0.0
}

fn pipeline_params(blend: bool, side: usize) -> PipelineParams {
    PipelineParams {
        cull_face: if side == 0 {
            CullFace::Nothing
        } else {
            CullFace::Back
        },
        front_face_order: if side == 2 {
            FrontFaceOrder::Clockwise
        } else {
            FrontFaceOrder::CounterClockwise
        },
        depth_test: Comparison::LessOrEqual,
        depth_write: !blend,
        color_blend: blend.then(|| {
            BlendState::new(
                Equation::Add,
                BlendFactor::Value(BlendValue::SourceAlpha),
                BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
            )
        }),
        alpha_blend: blend.then(|| {
            BlendState::new(
                Equation::Add,
                BlendFactor::One,
                BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
            )
        }),
        ..Default::default()
    }
}

const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
uniform mat4 Model;
uniform mat4 Projection;
varying mediump vec2 uv;
void main() {
    gl_Position = Projection * Model * vec4(position, 1.0);
    uv = texcoord;
}
"#;

const FRAGMENT: &str = r#"#version 100
precision mediump float;
varying mediump vec2 uv;
uniform sampler2D Texture;
uniform sampler2D EmissiveTexture;
uniform vec4 BaseColor;
uniform vec4 Emission;
uniform vec4 AlphaOutput;
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
void main() {
    vec4 base = texture2D(Texture, uv);
    float alpha = clamp(base.a * BaseColor.a, 0.0, 1.0);
    if (AlphaOutput.y > 0.5 && AlphaOutput.y < 1.5 && alpha < AlphaOutput.x) discard;
    vec3 rgb = decode_srgb(base.rgb) * BaseColor.rgb
             + decode_srgb(texture2D(EmissiveTexture, uv).rgb) * Emission.rgb;
    if (AlphaOutput.z > 0.5) rgb = encode_srgb(rgb);
    if (AlphaOutput.y < 1.5) alpha = 1.0;
    gl_FragColor = vec4(rgb, alpha);
}
"#;

const METAL: &str = r#"#include <metal_stdlib>
using namespace metal;
struct Vertex {
    float3 position [[attribute(0)]];
    float2 texcoord [[attribute(1)]];
};
struct RasterizerData {
    float4 position [[position]];
    float2 uv [[user(locn0)]];
};
struct Uniforms {
    float4x4 Model;
    float4x4 Projection;
    float4 _Time;
    float4 BaseColor;
    float4 Emission;
    float4 AlphaOutput;
};
vertex RasterizerData vertexShader(Vertex v [[stage_in]], constant Uniforms& u [[buffer(0)]]) {
    RasterizerData out;
    out.position = u.Projection * u.Model * float4(v.position, 1.0);
    out.uv = v.texcoord;
    return out;
}
float3 decode_srgb(float3 rgb) {
    return select(pow((rgb + 0.055) / 1.055, float3(2.4)), rgb / 12.92, rgb <= 0.04045);
}
float3 encode_srgb(float3 rgb) {
    rgb = max(rgb, float3(0.0));
    return select(1.055 * pow(rgb, float3(1.0 / 2.4)) - 0.055, rgb * 12.92, rgb <= 0.0031308);
}
fragment float4 fragmentShader(RasterizerData in [[stage_in]],
    constant Uniforms& u [[buffer(0)]],
    texture2d<float> Texture [[texture(0)]], sampler TextureSampler [[sampler(0)]],
    texture2d<float> EmissiveTexture [[texture(2)]], sampler EmissiveSampler [[sampler(2)]]) {
    float4 base = Texture.sample(TextureSampler, in.uv);
    float alpha = clamp(base.a * u.BaseColor.a, 0.0, 1.0);
    if (u.AlphaOutput.y > 0.5 && u.AlphaOutput.y < 1.5 && alpha < u.AlphaOutput.x) discard_fragment();
    float3 rgb = decode_srgb(base.rgb) * u.BaseColor.rgb
               + decode_srgb(EmissiveTexture.sample(EmissiveSampler, in.uv).rgb) * u.Emission.rgb;
    if (u.AlphaOutput.z > 0.5) rgb = encode_srgb(rgb);
    if (u.AlphaOutput.y < 1.5) alpha = 1.0;
    return float4(rgb, alpha);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_camera_and_transforms_without_gpu() {
        for direction in [Vec3::ZERO, Vec3::NAN, Vec3::splat(f32::INFINITY)] {
            assert!(checked_direction(Vec3::ZERO, direction).is_err());
        }
        assert!(checked_direction(Vec3::NAN, Vec3::Z).is_err());
        for scale in [f32::MIN_POSITIVE, 1., f32::MAX] {
            assert_eq!(
                checked_direction(Vec3::ZERO, -Vec3::Z * scale).unwrap(),
                -Vec3::Z
            );
        }
        for matrix in [
            Mat4::NAN,
            Mat4::ZERO,
            Mat4::perspective_rh_gl(1., 1., 0.1, 100.),
        ] {
            assert!(check_transform(matrix).is_err());
        }
        assert!(check_transform(Mat4::from_scale(Vec3::ZERO)).is_ok());
    }

    #[test]
    fn composed_reflections_and_large_scales_have_correct_handedness() {
        let reflection = Mat4::from_scale(Vec3::new(-1., 1., 1.));
        assert!(mirrored(reflection));
        assert!(!mirrored(reflection * reflection));
        assert!(mirrored(Mat4::from_scale(Vec3::new(-1e30, 1e30, 1e30))));
        assert!(!mirrored(Mat4::from_scale(Vec3::ZERO)));
        let mut shear = reflection;
        shear.y_axis += shear.x_axis * 2.;
        assert!(mirrored(shear));
    }

    #[test]
    fn depth_blend_and_sidedness_policies_are_independent() {
        for side in 0..3 {
            for blend in [false, true] {
                let params = pipeline_params(blend, side);
                assert_eq!(params.depth_test, Comparison::LessOrEqual);
                assert_eq!(params.depth_write, !blend);
                assert_eq!(params.color_blend.is_some(), blend);
                assert_eq!(params.alpha_blend.is_some(), blend);
                assert_eq!(params.cull_face == CullFace::Nothing, side == 0);
                assert_eq!(
                    params.front_face_order == FrontFaceOrder::Clockwise,
                    side == 2
                );
            }
        }
    }
}
