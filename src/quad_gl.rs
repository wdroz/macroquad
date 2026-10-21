//! Legacy module, code should be either removed or moved to different modules

use miniquad::*;

pub use miniquad::{FilterMode, TextureId as MiniquadTexture, UniformDesc};

use crate::{color::Color, logging::warn, telemetry, texture::Texture2D, tobytes::ToBytes, Error};

use std::{collections::BTreeMap, sync::Arc};

pub(crate) use crate::models::Vertex;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DrawMode {
    Triangles,
    Lines,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlPipeline(usize);

/// The renderer owns one reference until all public handles and queued draws
/// have released theirs. GPU deletion therefore never needs a global context
/// from a public handle's destructor.
#[derive(Debug)]
pub(crate) struct RetainedMeshBuffers {
    pub(crate) vertex_buffer: BufferId,
    pub(crate) index_buffer: BufferId,
    pub(crate) vertices_count: usize,
    pub(crate) indices_count: usize,
}

/// Extra glTF shading attributes packed for the lit normal/roughness shader.
///
/// The layout matches [`SHADING_ATTRIBUTES`]. `normal.w` is tangent handedness
/// and `tangent.xyz` is the node-world tangent. Map UVs are independent of the
/// base-color `uv`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct ShadingVertex {
    pub(crate) position: glam::Vec3,
    pub(crate) uv: glam::Vec2,
    pub(crate) normal: glam::Vec4,
    pub(crate) tangent: glam::Vec4,
    pub(crate) normal_uv: glam::Vec2,
    pub(crate) metallic_uv: glam::Vec2,
}

const _: () = assert!(std::mem::size_of::<ShadingVertex>() == 68);
const _: () = assert!(std::mem::offset_of!(ShadingVertex, uv) == 12);
const _: () = assert!(std::mem::offset_of!(ShadingVertex, normal) == 20);
const _: () = assert!(std::mem::offset_of!(ShadingVertex, tangent) == 36);
const _: () = assert!(std::mem::offset_of!(ShadingVertex, normal_uv) == 52);
const _: () = assert!(std::mem::offset_of!(ShadingVertex, metallic_uv) == 60);

#[cfg_attr(not(feature = "gltf"), allow(dead_code))]
pub(crate) const SHADING_ATTRIBUTES: [VertexAttribute; 6] = [
    VertexAttribute::new("position", VertexFormat::Float3),
    VertexAttribute::new("texcoord", VertexFormat::Float2),
    VertexAttribute::new("normal", VertexFormat::Float4),
    VertexAttribute::new("tangent", VertexFormat::Float4),
    VertexAttribute::new("normal_uv", VertexFormat::Float2),
    VertexAttribute::new("metallic_uv", VertexFormat::Float2),
];

const MESH_ATTRIBUTES: [VertexAttribute; 4] = [
    VertexAttribute::new("position", VertexFormat::Float3),
    VertexAttribute::new("texcoord", VertexFormat::Float2),
    VertexAttribute::new("color0", VertexFormat::Byte4),
    VertexAttribute::new("normal", VertexFormat::Float4),
];

/// Immutable shading vertices uploaded beside a retained glTF chunk.
#[derive(Debug)]
#[cfg_attr(not(feature = "gltf"), allow(dead_code))]
pub(crate) struct ShadingBuffers {
    pub(crate) vertex_buffer: BufferId,
    pub(crate) index_buffer: BufferId,
    pub(crate) vertices_count: usize,
    pub(crate) indices_count: usize,
}

struct DrawCall {
    vertices_count: usize,
    indices_count: usize,
    vertices_start: usize,
    indices_start: usize,

    clip: Option<(i32, i32, i32, i32)>,
    viewport: Option<(i32, i32, i32, i32)>,
    texture: Option<MiniquadTexture>,

    model: glam::Mat4,

    draw_mode: DrawMode,
    pipeline: GlPipeline,
    uniforms: Option<Vec<u8>>,
    // Preserve each material binding until this draw is submitted or cancelled.
    textures: BTreeMap<String, Texture2D>,
    render_pass: Option<RenderPass>,
    capture: bool,
    retained: Option<crate::models::RetainedMesh>,
    shading_stream: bool,
    shading_retained: Option<Arc<ShadingBuffers>>,
}

impl DrawCall {
    const fn new(
        texture: Option<miniquad::TextureId>,
        model: glam::Mat4,
        draw_mode: DrawMode,
        pipeline: GlPipeline,
        uniforms: Option<Vec<u8>>,
        render_pass: Option<RenderPass>,
    ) -> DrawCall {
        DrawCall {
            vertices_start: 0,
            indices_start: 0,
            vertices_count: 0,
            indices_count: 0,
            viewport: None,
            clip: None,
            texture,
            model,
            draw_mode,
            pipeline,
            uniforms,
            textures: BTreeMap::new(),
            render_pass,
            capture: false,
            retained: None,
            shading_stream: false,
            shading_retained: None,
        }
    }
}

struct MagicSnapshotter {
    pipeline: Pipeline,
    bindings: Bindings,
    pass: Option<RenderPass>,

    screen_texture: Option<miniquad::TextureId>,
}

mod snapshotter_shader {
    use miniquad::{ShaderMeta, UniformBlockLayout};

    pub const VERTEX: &str = r#"#version 100
    attribute vec2 position;
    attribute vec2 texcoord;

    varying lowp vec2 uv;

    void main() {
        gl_Position = vec4(position, 0, 1);
        uv = texcoord;
    }"#;

    pub const FRAGMENT: &str = r#"#version 100
    varying lowp vec2 uv;

    uniform sampler2D Texture;

    void main() {
        gl_FragColor = texture2D(Texture, uv);
    }"#;

    pub const METAL: &str = r#"#include <metal_stdlib>
    using namespace metal;

    struct Vertex
    {
        float2 position    [[attribute(0)]];
        float2 texcoord    [[attribute(1)]];
    };

    struct RasterizerData
    {
        float4 position [[position]];
        float2 uv [[user(locn1)]];
    };

    vertex RasterizerData vertexShader(Vertex v [[stage_in]])
    {
        RasterizerData out;

        out.position = float4(v.position, 0, 1);
        out.uv = v.texcoord;

        return out;
    }

    fragment float4 fragmentShader(RasterizerData in [[stage_in]], texture2d<float> tex [[texture(0)]], sampler texSmplr [[sampler(0)]])
    {
        return tex.sample(texSmplr, in.uv);
    }"#;

    pub fn meta() -> ShaderMeta {
        ShaderMeta {
            images: vec!["Texture".to_string()],
            uniforms: UniformBlockLayout { uniforms: vec![] },
        }
    }
}

impl MagicSnapshotter {
    fn new(ctx: &mut dyn RenderingBackend) -> MagicSnapshotter {
        let shader = ctx
            .new_shader(
                match ctx.info().backend {
                    Backend::OpenGl => ShaderSource::Glsl {
                        vertex: snapshotter_shader::VERTEX,
                        fragment: snapshotter_shader::FRAGMENT,
                    },
                    Backend::Metal => ShaderSource::Msl {
                        program: snapshotter_shader::METAL,
                    },
                },
                snapshotter_shader::meta(),
            )
            .unwrap_or_else(|e| panic!("Failed to load shader: {e}"));

        let pipeline = ctx.new_pipeline(
            &[BufferLayout::default()],
            &[
                VertexAttribute::new("position", VertexFormat::Float2),
                VertexAttribute::new("texcoord", VertexFormat::Float2),
            ],
            shader,
            PipelineParams::default(),
        );

        #[rustfmt::skip]
        let vertices: [f32; 16] = [
             -1.0, -1.0, 0., 0.,
             1.0, -1.0, 1., 0. ,
             1.0,  1.0, 1., 1. ,
            -1.0,  1.0, 0., 1. ,
        ];
        let vertex_buffer = ctx.new_buffer(
            BufferType::VertexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&vertices),
        );

        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
        let index_buffer = ctx.new_buffer(
            BufferType::IndexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&indices),
        );

        let bindings = Bindings {
            vertex_buffers: vec![vertex_buffer],
            index_buffer,
            images: vec![ctx.new_texture_from_rgba8(1, 1, &[0, 0, 0, 0])],
        };

        MagicSnapshotter {
            pipeline,
            bindings,
            pass: None,
            screen_texture: None,
        }
    }

    fn snapshot(
        &mut self,
        ctx: &mut dyn RenderingBackend,
        camera_render_pass: Option<RenderPass>,
    ) -> bool {
        if let Some(camera_render_pass) = camera_render_pass {
            let texture = ctx.render_pass_texture(camera_render_pass);
            if self.pass.is_none() {
                let miniquad::TextureParams {
                    width,
                    height,
                    format,
                    ..
                } = ctx.texture_params(texture);
                let color_img = ctx.new_render_texture(TextureParams {
                    width,
                    height,
                    format,
                    ..Default::default()
                });

                self.pass = Some(ctx.new_render_pass(color_img, None));
                self.screen_texture = Some(color_img);
            }

            if self.bindings.images.len() == 0 {
                self.bindings.images.push(texture);
            } else {
                self.bindings.images[0] = texture;
            }
            ctx.begin_pass(
                Some(self.pass.unwrap()),
                PassAction::clear_color(1.0, 0.0, 1.0, 1.),
            );
            ctx.apply_pipeline(&self.pipeline);
            ctx.apply_bindings(&self.bindings);
            ctx.draw(0, 6, 1);
            ctx.end_render_pass();
        } else {
            let (screen_width, screen_height) = miniquad::window::screen_size();
            if self.screen_texture.is_none()
                || self
                    .screen_texture
                    .map(|t| {
                        let (w, h) = ctx.texture_size(t);
                        w != screen_width as u32 || h != screen_height as u32
                    })
                    .unwrap_or(false)
            {
                self.screen_texture = Some(ctx.new_render_texture(TextureParams {
                    width: screen_width as _,
                    height: screen_height as _,
                    ..Default::default()
                }));
            }

            let texture = self.screen_texture.unwrap();
            Texture2D::unmanaged(texture).grab_screen();
        }
        camera_render_pass.is_some()
    }
}

struct GlState {
    texture: Option<miniquad::TextureId>,
    draw_mode: DrawMode,
    clip: Option<(i32, i32, i32, i32)>,
    clip_scope: Option<crate::clip_rect::ClipRect>,
    viewport: Option<(i32, i32, i32, i32)>,
    model_stack: Vec<glam::Mat4>,
    pipeline: Option<GlPipeline>,
    depth_test_enable: bool,

    break_batching: bool,
    snapshotter: MagicSnapshotter,

    render_pass: Option<RenderPass>,
    capture: bool,
}

impl GlState {
    fn model(&self) -> glam::Mat4 {
        *self.model_stack.last().unwrap()
    }
}

#[derive(Clone, Debug)]
struct Uniform {
    name: String,
    uniform_type: UniformType,
    byte_offset: usize,
    byte_size: usize,
}

#[derive(Clone)]
struct PipelineExt {
    pipeline: miniquad::Pipeline,
    depth_test: Comparison,
    depth_write: bool,
    wants_screen_texture: bool,
    uniforms: Vec<Uniform>,
    uniforms_data: Vec<u8>,
    textures: Vec<String>,
    textures_data: BTreeMap<String, Texture2D>,
}

impl PipelineExt {
    fn set_uniform<T>(&mut self, name: &str, uniform: T) {
        let uniform_meta = self.uniforms.iter().find(
            |Uniform {
                 name: uniform_name, ..
             }| uniform_name == name,
        );
        if uniform_meta.is_none() {
            warn!("Trying to set non-existing uniform: {}", name);
            return;
        }
        let uniform_meta = uniform_meta.unwrap();
        let uniform_format = uniform_meta.uniform_type;
        let uniform_byte_size = uniform_format.size();
        let uniform_byte_offset = uniform_meta.byte_offset;

        if size_of::<T>() != uniform_byte_size {
            warn!(
                "Trying to set uniform {} sized {} bytes value of {} bytes",
                name,
                uniform_byte_size,
                size_of::<T>()
            );
            return;
        }
        if uniform_byte_size != uniform_meta.byte_size {
            warn!("set_uniform do not support uniform arrays");
            return;
        }
        macro_rules! transmute_uniform {
            ($uniform_size:expr, $byte_offset:expr, $n:expr) => {
                if $uniform_size == $n {
                    let data: [u8; $n] = unsafe { std::mem::transmute_copy(&uniform) };

                    for i in 0..$uniform_size {
                        self.uniforms_data[$byte_offset + i] = data[i];
                    }
                }
            };
        }
        transmute_uniform!(uniform_byte_size, uniform_byte_offset, 4);
        transmute_uniform!(uniform_byte_size, uniform_byte_offset, 8);
        transmute_uniform!(uniform_byte_size, uniform_byte_offset, 12);
        transmute_uniform!(uniform_byte_size, uniform_byte_offset, 16);
        transmute_uniform!(uniform_byte_size, uniform_byte_offset, 64);
    }

    fn set_uniform_array<T: ToBytes>(&mut self, name: &str, uniform: &[T]) {
        let uniform_meta = self.uniforms.iter().find(
            |Uniform {
                 name: uniform_name, ..
             }| uniform_name == name,
        );
        if uniform_meta.is_none() {
            warn!("Trying to set non-existing uniform: {}", name);
            return;
        }
        let uniform_meta = uniform_meta.unwrap();
        let uniform_byte_size = uniform_meta.byte_size;
        let uniform_byte_offset = uniform_meta.byte_offset;

        let data = uniform.to_bytes();
        if data.len() != uniform_byte_size {
            warn!(
                "Trying to set uniform {} sized {} bytes value of {} bytes",
                name,
                uniform_byte_size,
                size_of::<T>()
            );
            return;
        }
        for i in 0..uniform_byte_size {
            self.uniforms_data[uniform_byte_offset + i] = data[i];
        }
    }
}

struct PipelinesStorage {
    pipelines: [Option<PipelineExt>; Self::MAX_PIPELINES],
    pipelines_amount: usize,
}

impl PipelinesStorage {
    const MAX_PIPELINES: usize = 32;
    const TRIANGLES_PIPELINE: GlPipeline = GlPipeline(0);
    const LINES_PIPELINE: GlPipeline = GlPipeline(1);
    const TRIANGLES_DEPTH_PIPELINE: GlPipeline = GlPipeline(2);
    const LINES_DEPTH_PIPELINE: GlPipeline = GlPipeline(3);

    fn new(ctx: &mut dyn RenderingBackend) -> PipelinesStorage {
        let shader = ctx
            .new_shader(
                match ctx.info().backend {
                    Backend::OpenGl => ShaderSource::Glsl {
                        vertex: shader::VERTEX,
                        fragment: shader::FRAGMENT,
                    },
                    Backend::Metal => ShaderSource::Msl {
                        program: shader::METAL,
                    },
                },
                shader::meta(),
            )
            .unwrap_or_else(|e| panic!("Failed to load shader: {e}"));

        let params = PipelineParams {
            color_blend: Some(BlendState::new(
                Equation::Add,
                BlendFactor::Value(BlendValue::SourceAlpha),
                BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
            )),
            // RGB uses straight source colors; coverage must use source-over
            // independently. Reusing SourceAlpha here squares glyph/shape
            // coverage and punches transparency into an opaque WebGL canvas.
            alpha_blend: Some(BlendState::new(
                Equation::Add,
                BlendFactor::One,
                BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
            )),
            ..Default::default()
        };

        let mut storage = PipelinesStorage {
            pipelines: Default::default(),
            pipelines_amount: 0,
        };

        let triangles_pipeline = storage.make_pipeline(
            ctx,
            shader,
            PipelineParams {
                primitive_type: PrimitiveType::Triangles,
                ..params
            },
            false,
            vec![],
            vec![],
            &MESH_ATTRIBUTES,
        );
        assert_eq!(triangles_pipeline, Self::TRIANGLES_PIPELINE);

        let lines_pipeline = storage.make_pipeline(
            ctx,
            shader,
            PipelineParams {
                primitive_type: PrimitiveType::Lines,
                ..params
            },
            false,
            vec![],
            vec![],
            &MESH_ATTRIBUTES,
        );
        assert_eq!(lines_pipeline, Self::LINES_PIPELINE);

        let triangles_depth_pipeline = storage.make_pipeline(
            ctx,
            shader,
            PipelineParams {
                depth_write: true,
                depth_test: Comparison::LessOrEqual,
                primitive_type: PrimitiveType::Triangles,
                ..params
            },
            false,
            vec![],
            vec![],
            &MESH_ATTRIBUTES,
        );
        assert_eq!(triangles_depth_pipeline, Self::TRIANGLES_DEPTH_PIPELINE);

        let lines_depth_pipeline = storage.make_pipeline(
            ctx,
            shader,
            PipelineParams {
                depth_write: true,
                depth_test: Comparison::LessOrEqual,
                primitive_type: PrimitiveType::Lines,
                ..params
            },
            false,
            vec![],
            vec![],
            &MESH_ATTRIBUTES,
        );
        assert_eq!(lines_depth_pipeline, Self::LINES_DEPTH_PIPELINE);

        storage
    }

    fn make_pipeline(
        &mut self,
        ctx: &mut dyn RenderingBackend,
        shader: ShaderId,
        params: PipelineParams,
        wants_screen_texture: bool,
        mut uniforms: Vec<UniformDesc>,
        textures: Vec<String>,
        attributes: &[VertexAttribute],
    ) -> GlPipeline {
        let pipeline = ctx.new_pipeline(&[BufferLayout::default()], attributes, shader, params);

        let id = self
            .pipelines
            .iter()
            .position(|p| p.is_none())
            .unwrap_or_else(|| panic!("Pipelines amount exceeded"));

        let mut max_offset = 0;

        for (name, kind) in shader::uniforms().into_iter().rev() {
            uniforms.insert(0, UniformDesc::new(name, kind));
        }

        let uniforms = uniforms
            .iter()
            .scan(0, |offset, uniform| {
                let byte_size = uniform.uniform_type.size() * uniform.array_count;
                let uniform = Uniform {
                    name: uniform.name.clone(),
                    uniform_type: uniform.uniform_type,
                    byte_size,
                    byte_offset: *offset,
                };
                *offset += byte_size;
                max_offset = *offset;

                Some(uniform)
            })
            .collect();

        self.pipelines[id] = Some(PipelineExt {
            pipeline,
            depth_test: params.depth_test,
            depth_write: params.depth_write,
            wants_screen_texture,
            uniforms,
            uniforms_data: vec![0; max_offset],
            textures,
            textures_data: BTreeMap::new(),
        });
        self.pipelines_amount += 1;

        GlPipeline(id)
    }

    const fn get(&self, draw_mode: DrawMode, depth_enabled: bool) -> GlPipeline {
        match (draw_mode, depth_enabled) {
            (DrawMode::Triangles, false) => Self::TRIANGLES_PIPELINE,
            (DrawMode::Triangles, true) => Self::TRIANGLES_DEPTH_PIPELINE,
            (DrawMode::Lines, false) => Self::LINES_PIPELINE,
            (DrawMode::Lines, true) => Self::LINES_DEPTH_PIPELINE,
        }
    }

    fn get_quad_pipeline_mut(&mut self, pip: GlPipeline) -> &mut PipelineExt {
        self.pipelines[pip.0].as_mut().unwrap()
    }

    fn delete_pipeline(&mut self, pip: GlPipeline) {
        self.pipelines[pip.0] = None;
    }
}

pub struct QuadGl {
    pipelines: PipelinesStorage,

    draw_calls: Vec<DrawCall>,
    draw_calls_bindings: Vec<Bindings>,
    draw_calls_count: usize,
    retained_meshes: Vec<Arc<RetainedMeshBuffers>>,
    // Scratch bindings hold no ownership of the GPU buffers. Every retained
    // draw replaces their IDs, and immediate draws use separate stream slots.
    retained_bindings: Option<Bindings>,
    shading_batch: Vec<ShadingVertex>,
    shading_bindings: Vec<Bindings>,
    shading_retained_bindings: Option<Bindings>,
    shading_meshes: Vec<Arc<ShadingBuffers>>,
    diagnostics_enabled: bool,
    diagnostics_counters: telemetry::RendererCounters,
    #[cfg(test)]
    retained_buffer_uploads: usize,
    #[cfg(test)]
    retained_buffer_deletions: usize,
    #[cfg(test)]
    stream_buffer_updates: usize,
    state: GlState,
    start_time: f64,

    pub(crate) white_texture: miniquad::TextureId,
    max_vertices: usize,
    max_indices: usize,

    batch_vertex_buffer: Vec<Vertex>,
    batch_index_buffer: Vec<u16>,
}

// A new slot starts at its actual geometry size. Reused slots grow
// geometrically, never past the configured draw-call limit, and never shrink
// between frames. Large mesh limits therefore do not penalize tiny meshes.
fn stream_buffer_capacity(current: usize, required: usize, maximum: usize) -> usize {
    debug_assert!(required <= maximum);
    if required <= current {
        current
    } else {
        required.max(current.saturating_mul(2)).min(maximum)
    }
}

fn grow_stream_buffer<T>(
    ctx: &mut dyn miniquad::RenderingBackend,
    buffer: &mut BufferId,
    buffer_type: BufferType,
    required: usize,
    maximum: usize,
) {
    let current = ctx.buffer_size(*buffer) / std::mem::size_of::<T>();
    let capacity = stream_buffer_capacity(current, required, maximum);
    if capacity > current {
        let previous = *buffer;
        *buffer = ctx.new_buffer(
            buffer_type,
            BufferUsage::Stream,
            BufferSource::empty::<T>(capacity),
        );
        ctx.delete_buffer(previous);
    }
}

impl QuadGl {
    pub fn new(
        ctx: &mut dyn miniquad::RenderingBackend,
        max_vertices: usize,
        max_indices: usize,
    ) -> QuadGl {
        let white_texture = ctx.new_texture_from_rgba8(1, 1, &[255, 255, 255, 255]);

        QuadGl {
            pipelines: PipelinesStorage::new(ctx),
            state: GlState {
                clip: None,
                clip_scope: None,
                viewport: None,
                texture: None,
                model_stack: vec![glam::Mat4::IDENTITY],
                draw_mode: DrawMode::Triangles,
                pipeline: None,
                break_batching: false,
                depth_test_enable: false,
                snapshotter: MagicSnapshotter::new(ctx),
                render_pass: None,
                capture: false,
            },
            draw_calls: Vec::with_capacity(200),
            draw_calls_bindings: Vec::with_capacity(200),
            draw_calls_count: 0,
            retained_meshes: Vec::new(),
            retained_bindings: None,
            shading_batch: Vec::new(),
            shading_bindings: Vec::new(),
            shading_retained_bindings: None,
            shading_meshes: Vec::new(),
            diagnostics_enabled: false,
            diagnostics_counters: telemetry::RendererCounters::default(),
            #[cfg(test)]
            retained_buffer_uploads: 0,
            #[cfg(test)]
            retained_buffer_deletions: 0,
            #[cfg(test)]
            stream_buffer_updates: 0,
            start_time: miniquad::date::now(),

            white_texture,
            batch_vertex_buffer: Vec::with_capacity(max_vertices),
            batch_index_buffer: Vec::with_capacity(max_indices),
            max_vertices,
            max_indices,
        }
    }

    pub fn make_pipeline(
        &mut self,
        ctx: &mut dyn miniquad::RenderingBackend,
        shader: miniquad::ShaderSource,
        params: PipelineParams,
        uniforms: Vec<UniformDesc>,
        textures: Vec<String>,
    ) -> Result<GlPipeline, Error> {
        let mut shader_meta: ShaderMeta = shader::meta();

        for uniform in &uniforms {
            shader_meta.uniforms.uniforms.push(uniform.clone());
        }

        for texture in &textures {
            if texture == "Texture" {
                panic!(
                    "you can't use name `Texture` for your texture. This name is reserved for the texture that will be drawn with that material"
                );
            }
            if texture == "_ScreenTexture" {
                panic!(
                    "you can't use name `_ScreenTexture` for your texture in shaders. This name is reserved for screen texture"
                );
            }
            shader_meta.images.push(texture.clone());
        }

        let source = match shader {
            ShaderSource::Glsl { fragment, .. } => fragment,
            ShaderSource::Msl { program } => program,
        };
        let wants_screen_texture = source.contains("_ScreenTexture");
        let shader = ctx.new_shader(shader, shader_meta)?;
        Ok(self.pipelines.make_pipeline(
            ctx,
            shader,
            params,
            wants_screen_texture,
            uniforms,
            textures,
            &MESH_ATTRIBUTES,
        ))
    }

    #[cfg_attr(not(feature = "gltf"), allow(dead_code))]
    pub(crate) fn make_pipeline_with_attributes(
        &mut self,
        ctx: &mut dyn miniquad::RenderingBackend,
        shader: miniquad::ShaderSource,
        params: PipelineParams,
        uniforms: Vec<UniformDesc>,
        textures: Vec<String>,
        attributes: &[VertexAttribute],
    ) -> Result<GlPipeline, Error> {
        let mut shader_meta: ShaderMeta = shader::meta();
        for uniform in &uniforms {
            shader_meta.uniforms.uniforms.push(uniform.clone());
        }
        for texture in &textures {
            if texture == "Texture" || texture == "_ScreenTexture" {
                panic!("texture name `{texture}` is reserved");
            }
            shader_meta.images.push(texture.clone());
        }
        let source = match shader {
            ShaderSource::Glsl { fragment, .. } => fragment,
            ShaderSource::Msl { program } => program,
        };
        let wants_screen_texture = source.contains("_ScreenTexture");
        let shader = ctx.new_shader(shader, shader_meta)?;
        Ok(self.pipelines.make_pipeline(
            ctx,
            shader,
            params,
            wants_screen_texture,
            uniforms,
            textures,
            attributes,
        ))
    }

    pub(crate) fn clear(&mut self, ctx: &mut dyn miniquad::RenderingBackend, color: Color) {
        let clear = PassAction::clear_color(color.r, color.g, color.b, color.a);

        if let Some(current_pass) = self.state.render_pass {
            ctx.begin_pass(Some(current_pass), clear);
        } else {
            ctx.begin_default_pass(clear);
        }
        ctx.end_render_pass();

        self.clear_draw_calls();
    }

    /// Reset only draw calls state
    pub fn clear_draw_calls(&mut self) {
        for dc in &mut self.draw_calls[..self.draw_calls_count] {
            dc.retained = None;
            dc.shading_stream = false;
            dc.shading_retained = None;
            dc.textures.clear();
        }
        self.draw_calls_count = 0;
        // Discard the geometry belonging to the cancelled calls too. Repeated
        // clears before a flush must not keep appending unreachable vertices.
        self.batch_vertex_buffer.clear();
        self.batch_index_buffer.clear();
        self.shading_batch.clear();
    }

    /// Reset internal state to known default
    pub fn reset(&mut self) {
        self.state.clip = None;
        self.state.clip_scope = None;
        self.state.texture = None;
        self.state.model_stack = vec![glam::Mat4::IDENTITY];

        self.clear_draw_calls();
    }

    pub fn draw(&mut self, ctx: &mut dyn miniquad::RenderingBackend, projection: glam::Mat4) {
        let white_texture = self.white_texture;

        // Only allocate slots actually submitted, at their current size. A
        // scene with hundreds of small material groups must not reserve the
        // configured maximum vertex/index capacity hundreds of times.
        let streamed = |dc: &&DrawCall| {
            dc.retained.is_none() && dc.shading_retained.is_none() && !dc.shading_stream
        };
        for dc in self
            .draw_calls
            .iter()
            .take(self.draw_calls_count)
            .filter(streamed)
            .skip(self.draw_calls_bindings.len())
        {
            let vertex_buffer = ctx.new_buffer(
                BufferType::VertexBuffer,
                BufferUsage::Stream,
                BufferSource::empty::<Vertex>(dc.vertices_count.max(1)),
            );
            let index_buffer = ctx.new_buffer(
                BufferType::IndexBuffer,
                BufferUsage::Stream,
                BufferSource::empty::<u16>(dc.indices_count.max(1)),
            );
            let bindings = Bindings {
                vertex_buffers: vec![vertex_buffer],
                index_buffer,
                images: vec![white_texture, white_texture],
            };

            self.draw_calls_bindings.push(bindings);
        }
        for dc in self
            .draw_calls
            .iter()
            .take(self.draw_calls_count)
            .filter(|dc| dc.shading_stream)
            .skip(self.shading_bindings.len())
        {
            let vertex_buffer = ctx.new_buffer(
                BufferType::VertexBuffer,
                BufferUsage::Stream,
                BufferSource::empty::<ShadingVertex>(dc.vertices_count.max(1)),
            );
            let index_buffer = ctx.new_buffer(
                BufferType::IndexBuffer,
                BufferUsage::Stream,
                BufferSource::empty::<u16>(dc.indices_count.max(1)),
            );
            self.shading_bindings.push(Bindings {
                vertex_buffers: vec![vertex_buffer],
                index_buffer,
                images: vec![white_texture, white_texture],
            });
        }
        debug_assert!(
            self.draw_calls_bindings.len()
                >= self.draw_calls[..self.draw_calls_count]
                    .iter()
                    .filter(streamed)
                    .count()
        );

        let (screen_width, screen_height) = miniquad::window::screen_size();
        let time = (miniquad::date::now() - self.start_time) as f32;
        let time = glam::vec4(time, time.sin(), time.cos(), 0.);
        let gl_depth_workaround = ctx.info().backend == Backend::OpenGl;

        let mut stream_slot = 0;
        let mut shading_slot = 0;
        for dc in &mut self.draw_calls[..self.draw_calls_count] {
            let bindings = if let Some(mesh) = &dc.shading_retained {
                let bindings = self
                    .shading_retained_bindings
                    .get_or_insert_with(|| Bindings {
                        vertex_buffers: vec![mesh.vertex_buffer],
                        index_buffer: mesh.index_buffer,
                        images: vec![white_texture, white_texture],
                    });
                bindings.vertex_buffers[0] = mesh.vertex_buffer;
                bindings.index_buffer = mesh.index_buffer;
                bindings
            } else if dc.shading_stream {
                let bindings = &mut self.shading_bindings[shading_slot];
                shading_slot += 1;
                grow_stream_buffer::<ShadingVertex>(
                    ctx,
                    &mut bindings.vertex_buffers[0],
                    BufferType::VertexBuffer,
                    dc.vertices_count,
                    self.max_vertices,
                );
                grow_stream_buffer::<u16>(
                    ctx,
                    &mut bindings.index_buffer,
                    BufferType::IndexBuffer,
                    dc.indices_count,
                    self.max_indices,
                );
                bindings
            } else if let Some(mesh) = &dc.retained {
                let bindings = self.retained_bindings.get_or_insert_with(|| Bindings {
                    vertex_buffers: vec![mesh.buffers.vertex_buffer],
                    index_buffer: mesh.buffers.index_buffer,
                    images: vec![white_texture, white_texture],
                });
                bindings.vertex_buffers[0] = mesh.buffers.vertex_buffer;
                bindings.index_buffer = mesh.buffers.index_buffer;
                bindings
            } else {
                let bindings = &mut self.draw_calls_bindings[stream_slot];
                stream_slot += 1;
                grow_stream_buffer::<Vertex>(
                    ctx,
                    &mut bindings.vertex_buffers[0],
                    BufferType::VertexBuffer,
                    dc.vertices_count,
                    self.max_vertices,
                );
                grow_stream_buffer::<u16>(
                    ctx,
                    &mut bindings.index_buffer,
                    BufferType::IndexBuffer,
                    dc.indices_count,
                    self.max_indices,
                );
                bindings
            };
            let pipeline = self.pipelines.get_quad_pipeline_mut(dc.pipeline);

            let (width, height) = if let Some(render_pass) = dc.render_pass {
                let render_texture = ctx.render_pass_texture(render_pass);
                let (width, height) = ctx.texture_size(render_texture);
                (width, height)
            } else {
                (screen_width as u32, screen_height as u32)
            };

            if pipeline.wants_screen_texture {
                let submitted = self.state.snapshotter.snapshot(ctx, dc.render_pass);
                if submitted && self.diagnostics_enabled {
                    let counters = &mut self.diagnostics_counters;
                    counters.draw_calls = counters.draw_calls.saturating_add(1);
                    counters.auxiliary_draw_calls = counters.auxiliary_draw_calls.saturating_add(1);
                }
            }

            if let Some(render_pass) = dc.render_pass {
                ctx.begin_pass(Some(render_pass), PassAction::Nothing);
            } else {
                ctx.begin_default_pass(PassAction::Nothing);
            }

            if dc.shading_stream {
                let vertex_bytes = dc.vertices_count * std::mem::size_of::<ShadingVertex>();
                ctx.buffer_update(
                    bindings.vertex_buffers[0],
                    BufferSource::slice(
                        &self.shading_batch
                            [dc.vertices_start..(dc.vertices_start + dc.vertices_count)],
                    ),
                );
                ctx.buffer_update(
                    bindings.index_buffer,
                    BufferSource::slice(
                        &self.batch_index_buffer
                            [dc.indices_start..(dc.indices_start + dc.indices_count)],
                    ),
                );
                #[cfg(test)]
                {
                    self.stream_buffer_updates += 2;
                }
                if self.diagnostics_enabled {
                    let counters = &mut self.diagnostics_counters;
                    counters.streaming_buffer_uploads =
                        counters.streaming_buffer_uploads.saturating_add(2);
                    counters.streaming_buffer_upload_bytes =
                        counters.streaming_buffer_upload_bytes.saturating_add(
                            (vertex_bytes + dc.indices_count * std::mem::size_of::<u16>()) as u64,
                        );
                }
            } else if dc.retained.is_none() && dc.shading_retained.is_none() {
                ctx.buffer_update(
                    bindings.vertex_buffers[0],
                    BufferSource::slice(
                        &self.batch_vertex_buffer
                            [dc.vertices_start..(dc.vertices_start + dc.vertices_count)],
                    ),
                );
                ctx.buffer_update(
                    bindings.index_buffer,
                    BufferSource::slice(
                        &self.batch_index_buffer
                            [dc.indices_start..(dc.indices_start + dc.indices_count)],
                    ),
                );
                #[cfg(test)]
                {
                    self.stream_buffer_updates += 2;
                }
                if self.diagnostics_enabled {
                    let counters = &mut self.diagnostics_counters;
                    counters.streaming_buffer_uploads =
                        counters.streaming_buffer_uploads.saturating_add(2);
                    counters.streaming_buffer_upload_bytes =
                        counters.streaming_buffer_upload_bytes.saturating_add(
                            (dc.vertices_count * std::mem::size_of::<Vertex>()
                                + dc.indices_count * std::mem::size_of::<u16>())
                                as u64,
                        );
                }
            }

            bindings.images[0] = dc.texture.unwrap_or(white_texture);
            bindings.images[1] = self
                .state
                .snapshotter
                .screen_texture
                .unwrap_or(white_texture);
            bindings
                .images
                .resize(2 + pipeline.textures.len(), white_texture);

            for (pos, name) in pipeline.textures.iter().enumerate() {
                bindings.images[2 + pos] = dc
                    .textures
                    .get(name)
                    .map(Texture2D::raw_miniquad_id)
                    .unwrap_or(white_texture);
            }

            ctx.apply_pipeline(&pipeline.pipeline);
            // miniquad 0.4.11's GL backend uses depth_write to enable the
            // depth test and never sets the write mask. Honor the independent
            // PipelineParams here, especially for depth-tested transparent 3D.
            if gl_depth_workaround {
                unsafe {
                    if pipeline.depth_write || pipeline.depth_test != Comparison::Always {
                        gl::glEnable(gl::GL_DEPTH_TEST);
                        gl::glDepthFunc(pipeline.depth_test.into());
                    } else {
                        gl::glDisable(gl::GL_DEPTH_TEST);
                    }
                    gl::glDepthMask(pipeline.depth_write as u8);
                }
            }
            if let Some((x, y, w, h)) = dc.viewport {
                ctx.apply_viewport(x, y, w, h);
            } else {
                ctx.apply_viewport(0, 0, width as i32, height as i32);
            }
            if let Some(clip) = dc.clip {
                ctx.apply_scissor_rect(clip.0, height as i32 - (clip.1 + clip.3), clip.2, clip.3);
            } else {
                ctx.apply_scissor_rect(0, 0, width as i32, height as i32);
            }
            ctx.apply_bindings(bindings);

            if let Some(ref uniforms) = dc.uniforms {
                for i in 0..uniforms.len() {
                    pipeline.uniforms_data[i] = uniforms[i];
                }
            }
            pipeline.set_uniform("Projection", projection);
            pipeline.set_uniform("Model", dc.model);
            pipeline.set_uniform("_Time", time);
            ctx.apply_uniforms_from_bytes(
                pipeline.uniforms_data.as_ptr(),
                pipeline.uniforms_data.len(),
            );
            ctx.draw(0, dc.indices_count as i32, 1);
            if self.diagnostics_enabled {
                let counters = &mut self.diagnostics_counters;
                counters.draw_calls = counters.draw_calls.saturating_add(1);
                if dc.retained.is_some() || dc.shading_retained.is_some() {
                    counters.retained_draw_calls = counters.retained_draw_calls.saturating_add(1);
                } else {
                    counters.immediate_draw_calls = counters.immediate_draw_calls.saturating_add(1);
                }
            }
            // Leave miniquad's expected default mask intact for clears and
            // direct backend users between macroquad batches.
            if gl_depth_workaround {
                unsafe { gl::glDepthMask(1) };
            }
            ctx.end_render_pass();

            if dc.capture {
                telemetry::track_drawcall(&pipeline.pipeline, bindings, dc.indices_count);
                if self.diagnostics_enabled {
                    let counters = &mut self.diagnostics_counters;
                    counters.draw_calls = counters.draw_calls.saturating_add(1);
                    counters.auxiliary_draw_calls = counters.auxiliary_draw_calls.saturating_add(1);
                }
            }

            dc.vertices_count = 0;
            dc.indices_count = 0;
            dc.vertices_start = 0;
            dc.indices_start = 0;
            dc.retained = None;
            dc.shading_stream = false;
            dc.shading_retained = None;
            // Draw-call slots are cached, but their texture ownership lasts
            // only until submission. Keep unused slots from pinning old assets.
            dc.textures.clear();
        }

        self.draw_calls_count = 0;
        self.batch_index_buffer.clear();
        self.batch_vertex_buffer.clear();
        self.shading_batch.clear();
        self.collect_retained_meshes(ctx);
        self.collect_shading_meshes(ctx);
    }

    pub(crate) fn capture(&mut self, capture: bool) {
        self.state.capture = capture;
    }

    pub fn get_projection_matrix(&self) -> glam::Mat4 {
        // get_projection_matrix is a way plugins used to get macroquad's current projection
        // back in the days when projection was a part of static batcher
        // now it is not, so here we go with this hack

        crate::get_context().projection_matrix()
    }

    pub const fn get_active_render_pass(&self) -> Option<RenderPass> {
        self.state.render_pass
    }

    pub const fn is_depth_test_enabled(&self) -> bool {
        self.state.depth_test_enable
    }

    pub fn render_pass(&mut self, render_pass: Option<RenderPass>) {
        self.state.render_pass = render_pass;
    }

    pub fn depth_test(&mut self, enable: bool) {
        self.state.depth_test_enable = enable;
    }

    pub fn texture(&mut self, texture: Option<&Texture2D>) {
        let ctx = crate::get_context();
        self.state.texture = texture.map(|t| ctx.raw_miniquad_id(&t.texture));
    }

    pub fn scissor(&mut self, clip: Option<(i32, i32, i32, i32)>) {
        self.state.clip = clip;
    }

    pub(crate) fn scissor_state(&self) -> Option<crate::clip_rect::Scissor> {
        self.state.clip
    }

    pub(crate) fn clip_scope(&self) -> Option<crate::clip_rect::ClipRect> {
        self.state.clip_scope
    }

    pub(crate) fn set_clip_scope(&mut self, clip: Option<crate::clip_rect::ClipRect>) {
        self.state.clip_scope = clip;
    }

    pub(crate) fn viewport_state(&self) -> Option<crate::clip_rect::Scissor> {
        self.state.viewport
    }

    pub fn viewport(&mut self, viewport: Option<(i32, i32, i32, i32)>) {
        self.state.viewport = viewport;
    }

    pub fn get_viewport(&self) -> (i32, i32, i32, i32) {
        self.state.viewport.unwrap_or((
            0,
            0,
            crate::window::screen_width() as _,
            crate::window::screen_height() as _,
        ))
    }

    pub fn push_model_matrix(&mut self, matrix: glam::Mat4) {
        self.state.model_stack.push(self.state.model() * matrix);
    }

    #[cfg(feature = "gltf")]
    pub(crate) fn model_matrix(&self) -> glam::Mat4 {
        self.state.model()
    }

    pub fn pop_model_matrix(&mut self) {
        if self.state.model_stack.len() > 1 {
            self.state.model_stack.pop();
        }
    }

    pub fn pipeline(&mut self, pipeline: Option<GlPipeline>) {
        if self.state.pipeline == pipeline {
            return;
        }

        self.state.break_batching = true;
        self.state.pipeline = pipeline;
    }

    pub fn draw_mode(&mut self, mode: DrawMode) {
        self.state.draw_mode = mode;
    }

    pub(crate) fn upload_retained_mesh(
        &mut self,
        ctx: &mut dyn RenderingBackend,
        vertices: &[Vertex],
        indices: &[u16],
    ) -> Arc<RetainedMeshBuffers> {
        self.collect_retained_meshes(ctx);
        let buffers = Arc::new(RetainedMeshBuffers {
            vertex_buffer: ctx.new_buffer(
                BufferType::VertexBuffer,
                BufferUsage::Immutable,
                BufferSource::slice(vertices),
            ),
            index_buffer: ctx.new_buffer(
                BufferType::IndexBuffer,
                BufferUsage::Immutable,
                BufferSource::slice(indices),
            ),
            vertices_count: vertices.len(),
            indices_count: indices.len(),
        });
        #[cfg(test)]
        {
            self.retained_buffer_uploads += 2;
        }
        if self.diagnostics_enabled {
            let counters = &mut self.diagnostics_counters;
            counters.retained_buffer_uploads = counters.retained_buffer_uploads.saturating_add(2);
            counters.retained_buffer_upload_bytes =
                counters.retained_buffer_upload_bytes.saturating_add(
                    (std::mem::size_of_val(vertices) + std::mem::size_of_val(indices)) as u64,
                );
        }
        self.retained_meshes.push(buffers.clone());
        buffers
    }

    #[cfg_attr(not(feature = "gltf"), allow(dead_code))]
    pub(crate) fn upload_shading_mesh(
        &mut self,
        ctx: &mut dyn RenderingBackend,
        vertices: &[ShadingVertex],
        indices: &[u16],
    ) -> Arc<ShadingBuffers> {
        self.collect_shading_meshes(ctx);
        let buffers = Arc::new(ShadingBuffers {
            vertex_buffer: ctx.new_buffer(
                BufferType::VertexBuffer,
                BufferUsage::Immutable,
                BufferSource::slice(vertices),
            ),
            index_buffer: ctx.new_buffer(
                BufferType::IndexBuffer,
                BufferUsage::Immutable,
                BufferSource::slice(indices),
            ),
            vertices_count: vertices.len(),
            indices_count: indices.len(),
        });
        #[cfg(test)]
        {
            self.retained_buffer_uploads += 2;
        }
        if self.diagnostics_enabled {
            let counters = &mut self.diagnostics_counters;
            counters.retained_buffer_uploads = counters.retained_buffer_uploads.saturating_add(2);
            counters.retained_buffer_upload_bytes =
                counters.retained_buffer_upload_bytes.saturating_add(
                    (std::mem::size_of_val(vertices) + std::mem::size_of_val(indices)) as u64,
                );
        }
        self.shading_meshes.push(buffers.clone());
        buffers
    }

    fn collect_shading_meshes(&mut self, ctx: &mut dyn RenderingBackend) {
        self.shading_meshes.retain(|buffers| {
            if Arc::strong_count(buffers) == 1 {
                ctx.delete_buffer(buffers.vertex_buffer);
                ctx.delete_buffer(buffers.index_buffer);
                #[cfg(test)]
                {
                    self.retained_buffer_deletions += 2;
                }
                false
            } else {
                true
            }
        });
    }

    fn collect_retained_meshes(&mut self, ctx: &mut dyn RenderingBackend) {
        self.retained_meshes.retain(|buffers| {
            if Arc::strong_count(buffers) == 1 {
                ctx.delete_buffer(buffers.vertex_buffer);
                ctx.delete_buffer(buffers.index_buffer);
                #[cfg(test)]
                {
                    self.retained_buffer_deletions += 2;
                }
                false
            } else {
                true
            }
        });
    }

    pub(crate) fn geometry_retained(
        &mut self,
        mesh: &crate::models::RetainedMesh,
        transform: glam::Mat4,
    ) {
        let pip = self.state.pipeline.unwrap_or(
            self.pipelines
                .get(DrawMode::Triangles, self.state.depth_test_enable),
        );
        let uniforms = self.state.pipeline.map(|pipeline| {
            self.pipelines
                .get_quad_pipeline_mut(pipeline)
                .uniforms_data
                .clone()
        });
        let texture = mesh.texture.as_ref().map(Texture2D::raw_miniquad_id);
        let model = self.state.model() * transform;
        if self.draw_calls_count == self.draw_calls.len() {
            self.draw_calls.push(DrawCall::new(
                texture,
                model,
                DrawMode::Triangles,
                pip,
                None,
                self.state.render_pass,
            ));
        }
        let dc = &mut self.draw_calls[self.draw_calls_count];
        dc.vertices_start = 0;
        dc.indices_start = 0;
        dc.vertices_count = mesh.buffers.vertices_count;
        dc.indices_count = mesh.buffers.indices_count;
        dc.clip = self.state.clip;
        dc.viewport = self.state.viewport;
        dc.texture = texture;
        dc.model = model;
        dc.draw_mode = DrawMode::Triangles;
        dc.pipeline = pip;
        dc.uniforms = uniforms;
        dc.textures = self
            .pipelines
            .get_quad_pipeline_mut(pip)
            .textures_data
            .clone();
        dc.render_pass = self.state.render_pass;
        dc.capture = self.state.capture;
        dc.retained = Some(mesh.clone());
        dc.shading_stream = false;
        dc.shading_retained = None;
        self.draw_calls_count += 1;
        self.state.break_batching = false;
    }

    #[cfg_attr(not(feature = "gltf"), allow(dead_code))]
    pub(crate) fn stream_limits(&self) -> (usize, usize) {
        (self.max_vertices, self.max_indices)
    }

    #[cfg_attr(not(feature = "gltf"), allow(dead_code))]
    pub(crate) fn geometry_shading(&mut self, vertices: &[ShadingVertex], indices: &[u16]) {
        if vertices.is_empty() || indices.is_empty() {
            return;
        }
        if vertices.len() > self.max_vertices || indices.len() > self.max_indices {
            warn!("shading geometry exceeded max drawcall size, skipping");
            return;
        }
        let pip = self.state.pipeline.unwrap_or(
            self.pipelines
                .get(DrawMode::Triangles, self.state.depth_test_enable),
        );
        let uniforms = self.state.pipeline.map(|pipeline| {
            self.pipelines
                .get_quad_pipeline_mut(pipeline)
                .uniforms_data
                .clone()
        });
        if self.draw_calls_count >= self.draw_calls.len() {
            self.draw_calls.push(DrawCall::new(
                self.state.texture,
                self.state.model(),
                DrawMode::Triangles,
                pip,
                None,
                self.state.render_pass,
            ));
        }
        let vertices_start = self.shading_batch.len();
        let indices_start = self.batch_index_buffer.len();
        self.shading_batch.extend_from_slice(vertices);
        self.batch_index_buffer.extend_from_slice(indices);
        let dc = &mut self.draw_calls[self.draw_calls_count];
        dc.vertices_start = vertices_start;
        dc.indices_start = indices_start;
        dc.vertices_count = vertices.len();
        dc.indices_count = indices.len();
        dc.clip = self.state.clip;
        dc.viewport = self.state.viewport;
        dc.texture = self.state.texture;
        dc.model = self.state.model();
        dc.draw_mode = DrawMode::Triangles;
        dc.pipeline = pip;
        dc.uniforms = uniforms;
        dc.textures = self
            .pipelines
            .get_quad_pipeline_mut(pip)
            .textures_data
            .clone();
        dc.render_pass = self.state.render_pass;
        dc.capture = self.state.capture;
        dc.retained = None;
        dc.shading_stream = true;
        dc.shading_retained = None;
        self.draw_calls_count += 1;
        self.state.break_batching = true;
    }

    #[cfg_attr(not(feature = "gltf"), allow(dead_code))]
    pub(crate) fn geometry_retained_shading(
        &mut self,
        mesh: &Arc<ShadingBuffers>,
        texture: Option<MiniquadTexture>,
        transform: glam::Mat4,
    ) {
        let pip = self.state.pipeline.unwrap_or(
            self.pipelines
                .get(DrawMode::Triangles, self.state.depth_test_enable),
        );
        let uniforms = self.state.pipeline.map(|pipeline| {
            self.pipelines
                .get_quad_pipeline_mut(pipeline)
                .uniforms_data
                .clone()
        });
        let model = self.state.model() * transform;
        if self.draw_calls_count >= self.draw_calls.len() {
            self.draw_calls.push(DrawCall::new(
                texture,
                model,
                DrawMode::Triangles,
                pip,
                None,
                self.state.render_pass,
            ));
        }
        let dc = &mut self.draw_calls[self.draw_calls_count];
        dc.vertices_start = 0;
        dc.indices_start = 0;
        dc.vertices_count = mesh.vertices_count;
        dc.indices_count = mesh.indices_count;
        dc.clip = self.state.clip;
        dc.viewport = self.state.viewport;
        dc.texture = texture;
        dc.model = model;
        dc.draw_mode = DrawMode::Triangles;
        dc.pipeline = pip;
        dc.uniforms = uniforms;
        dc.textures = self
            .pipelines
            .get_quad_pipeline_mut(pip)
            .textures_data
            .clone();
        dc.render_pass = self.state.render_pass;
        dc.capture = self.state.capture;
        dc.retained = None;
        dc.shading_stream = false;
        dc.shading_retained = Some(Arc::clone(mesh));
        self.draw_calls_count += 1;
        self.state.break_batching = true;
    }

    pub fn geometry(&mut self, vertices: &[Vertex], indices: &[u16]) {
        if vertices.len() >= self.max_vertices || indices.len() >= self.max_indices {
            warn!("geometry() exceeded max drawcall size, clamping");
        }

        let vertices = &vertices[0..self.max_vertices.min(vertices.len())];
        let indices = &indices[0..self.max_indices.min(indices.len())];

        let pip = self.state.pipeline.unwrap_or(
            self.pipelines
                .get(self.state.draw_mode, self.state.depth_test_enable),
        );

        let previous_dc_ix = if self.draw_calls_count == 0 {
            None
        } else {
            Some(self.draw_calls_count - 1)
        };
        let previous_dc = previous_dc_ix.and_then(|ix| self.draw_calls.get(ix));

        if previous_dc.map_or(true, |draw_call| {
            draw_call.retained.is_some()
                || draw_call.shading_stream
                || draw_call.shading_retained.is_some()
                || draw_call.texture != self.state.texture
                || draw_call.clip != self.state.clip
                || draw_call.viewport != self.state.viewport
                || draw_call.model != self.state.model()
                || draw_call.pipeline != pip
                || draw_call.render_pass != self.state.render_pass
                || draw_call.draw_mode != self.state.draw_mode
                || draw_call.vertices_count >= self.max_vertices - vertices.len()
                || draw_call.indices_count >= self.max_indices - indices.len()
                || draw_call.capture != self.state.capture
                || self.state.break_batching
        }) {
            let uniforms = self.state.pipeline.map_or(None, |pipeline| {
                Some(
                    self.pipelines
                        .get_quad_pipeline_mut(pipeline)
                        .uniforms_data
                        .clone(),
                )
            });

            if self.draw_calls_count >= self.draw_calls.len() {
                self.draw_calls.push(DrawCall::new(
                    self.state.texture,
                    self.state.model(),
                    self.state.draw_mode,
                    pip,
                    uniforms.clone(),
                    self.state.render_pass,
                ));
            }
            self.draw_calls[self.draw_calls_count].texture = self.state.texture;
            self.draw_calls[self.draw_calls_count].retained = None;
            self.draw_calls[self.draw_calls_count].shading_stream = false;
            self.draw_calls[self.draw_calls_count].shading_retained = None;
            self.draw_calls[self.draw_calls_count].uniforms = uniforms;
            self.draw_calls[self.draw_calls_count].textures = self
                .pipelines
                .get_quad_pipeline_mut(pip)
                .textures_data
                .clone();
            self.draw_calls[self.draw_calls_count].vertices_count = 0;
            self.draw_calls[self.draw_calls_count].indices_count = 0;
            self.draw_calls[self.draw_calls_count].clip = self.state.clip;
            self.draw_calls[self.draw_calls_count].viewport = self.state.viewport;
            self.draw_calls[self.draw_calls_count].model = self.state.model();
            self.draw_calls[self.draw_calls_count].draw_mode = self.state.draw_mode;
            self.draw_calls[self.draw_calls_count].pipeline = pip;
            self.draw_calls[self.draw_calls_count].render_pass = self.state.render_pass;
            self.draw_calls[self.draw_calls_count].capture = self.state.capture;
            self.draw_calls[self.draw_calls_count].indices_start = self.batch_index_buffer.len();
            self.draw_calls[self.draw_calls_count].vertices_start = self.batch_vertex_buffer.len();

            self.draw_calls_count += 1;
            self.state.break_batching = false;
        };
        let dc = &mut self.draw_calls[self.draw_calls_count - 1];

        self.batch_vertex_buffer.extend(vertices);
        self.batch_index_buffer
            .extend(indices.iter().map(|x| *x + dc.vertices_count as u16));

        dc.vertices_count += vertices.len();
        dc.indices_count += indices.len();

        dc.texture = self.state.texture;
    }

    pub fn delete_pipeline(&mut self, pipeline: GlPipeline) {
        self.pipelines.delete_pipeline(pipeline);
    }

    pub fn set_uniform<T>(&mut self, pipeline: GlPipeline, name: &str, uniform: T) {
        self.state.break_batching = true;

        self.pipelines
            .get_quad_pipeline_mut(pipeline)
            .set_uniform(name, uniform);
    }
    pub fn set_uniform_array<T: ToBytes>(
        &mut self,
        pipeline: GlPipeline,
        name: &str,
        uniform: &[T],
    ) {
        self.state.break_batching = true;

        self.pipelines
            .get_quad_pipeline_mut(pipeline)
            .set_uniform_array(name, uniform);
    }

    pub fn set_texture(&mut self, pipeline: GlPipeline, name: &str, texture: Texture2D) {
        self.state.break_batching = true;
        let pipeline = self.pipelines.get_quad_pipeline_mut(pipeline);
        pipeline
            .textures
            .iter()
            .find(|x| *x == name)
            .unwrap_or_else(|| {
                panic!(
                    "can't find texture with name '{}', there is only this names: {:?}",
                    name, pipeline.textures
                )
            });
        pipeline.textures_data.insert(name.to_owned(), texture);
    }

    pub(crate) fn update_drawcall_capacity(
        &mut self,
        ctx: &mut dyn miniquad::RenderingBackend,
        max_vertices: usize,
        max_indices: usize,
    ) {
        self.max_vertices = max_vertices;
        self.max_indices = max_indices;
        self.clear_draw_calls();
        self.collect_retained_meshes(ctx);
        self.collect_shading_meshes(ctx);
        self.trim_drawcall_buffers(ctx);
    }

    // The caller must submit or explicitly cancel pending work first. This
    // releases cache allocations without changing draw limits or live state.
    pub(crate) fn trim_drawcall_buffers(&mut self, ctx: &mut dyn RenderingBackend) {
        debug_assert_eq!(self.draw_calls_count, 0);
        debug_assert!(self.batch_vertex_buffer.is_empty());
        debug_assert!(self.batch_index_buffer.is_empty());
        self.batch_vertex_buffer = Vec::new();
        self.batch_index_buffer = Vec::new();
        self.draw_calls = Vec::new();
        self.retained_bindings = None;
        self.shading_retained_bindings = None;
        self.retained_meshes.shrink_to_fit();
        self.shading_meshes.shrink_to_fit();
        self.shading_batch = Vec::new();
        // Recreate only the slots needed by the next draw, at that draw's size.
        for binding in self
            .draw_calls_bindings
            .drain(..)
            .chain(self.shading_bindings.drain(..))
        {
            ctx.delete_buffer(binding.index_buffer);
            for vertex_buffer in &binding.vertex_buffers {
                ctx.delete_buffer(*vertex_buffer);
            }
        }
        self.draw_calls_bindings = Vec::new();
        self.shading_bindings = Vec::new();
    }

    pub(crate) fn set_diagnostics_enabled(&mut self, enabled: bool) {
        self.diagnostics_enabled = enabled;
    }

    pub(crate) fn reset_diagnostics(&mut self) {
        self.diagnostics_counters = telemetry::RendererCounters::default();
    }

    pub(crate) fn diagnostics(
        &self,
        ctx: &mut dyn RenderingBackend,
    ) -> telemetry::RendererDiagnostics {
        telemetry::RendererDiagnostics {
            counters_enabled: self.diagnostics_enabled,
            counters: self.diagnostics_counters,
            queued_draw_calls: self.draw_calls_count,
            cpu_vertex_bytes: self.batch_vertex_buffer.len() * std::mem::size_of::<Vertex>()
                + self.shading_batch.len() * std::mem::size_of::<ShadingVertex>(),
            cpu_vertex_capacity_bytes: self.batch_vertex_buffer.capacity()
                * std::mem::size_of::<Vertex>()
                + self.shading_batch.capacity() * std::mem::size_of::<ShadingVertex>(),
            cpu_index_bytes: self.batch_index_buffer.len() * std::mem::size_of::<u16>(),
            cpu_index_capacity_bytes: self.batch_index_buffer.capacity()
                * std::mem::size_of::<u16>(),
            retained_mesh_count: self.retained_meshes.len() + self.shading_meshes.len(),
            retained_gpu_buffer_count: (self.retained_meshes.len() + self.shading_meshes.len()) * 2,
            estimated_retained_gpu_buffer_bytes: self
                .retained_meshes
                .iter()
                .map(|buffers| {
                    ctx.buffer_size(buffers.vertex_buffer) + ctx.buffer_size(buffers.index_buffer)
                })
                .sum::<usize>()
                + self
                    .shading_meshes
                    .iter()
                    .map(|buffers| {
                        ctx.buffer_size(buffers.vertex_buffer)
                            + ctx.buffer_size(buffers.index_buffer)
                    })
                    .sum::<usize>(),
            streaming_gpu_buffer_count: (self.draw_calls_bindings.len()
                + self.shading_bindings.len())
                * 2,
            estimated_streaming_gpu_buffer_capacity_bytes: self
                .draw_calls_bindings
                .iter()
                .chain(self.shading_bindings.iter())
                .map(|bindings| {
                    ctx.buffer_size(bindings.vertex_buffers[0])
                        + ctx.buffer_size(bindings.index_buffer)
                })
                .sum(),
        }
    }
}

#[cfg(test)]
mod stream_buffer_tests {
    use super::*;

    #[test]
    fn small_draws_do_not_reserve_the_configured_mesh_limit() {
        let vertices = stream_buffer_capacity(0, 24, 20_001);
        let indices = stream_buffer_capacity(0, 36, 60_001);
        assert_eq!((vertices, indices), (24, 36));
        let actual = 398 * (vertices * std::mem::size_of::<Vertex>() + indices * 2);
        let worst_case = 398 * (20_001 * std::mem::size_of::<Vertex>() + 60_001 * 2);
        assert!(actual < worst_case / 100);
    }

    #[test]
    fn reused_slots_grow_geometrically_without_exceeding_the_limit() {
        assert_eq!(stream_buffer_capacity(24, 25, 100), 48);
        assert_eq!(stream_buffer_capacity(48, 25, 100), 48);
        assert_eq!(stream_buffer_capacity(48, 99, 100), 99);
        assert_eq!(stream_buffer_capacity(99, 100, 100), 100);
        assert_eq!(stream_buffer_capacity(100, 1, 100), 100);
        assert_eq!(
            stream_buffer_capacity(usize::MAX / 2 + 1, usize::MAX, usize::MAX),
            usize::MAX
        );
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod retained_buffer_tests {
    use super::*;
    use crate::models::{draw_mesh, draw_retained_mesh, Mesh, RetainedMesh};

    #[cfg(feature = "gltf")]
    #[test]
    #[ignore = "requires a native graphics context"]
    fn retained_gltf_uploads_are_shared_across_instances_and_frames() {
        use crate::gltf_draw::{GltfModel, RetainedGltfModel};
        use glam::{Mat4, Vec3};

        crate::Window::new("retained glTF upload counts", async {
            let source =
                GltfModel::from_bytes(include_bytes!("../examples/assets/textured_quad.glb"))
                    .unwrap();
            let chunks = source.parts.len();
            let texture = source.parts[0]
                .mesh
                .texture
                .as_ref()
                .unwrap()
                .raw_miniquad_id();
            // A bad later chunk must fail before uploading an earlier valid one.
            let mut invalid = source.clone();
            let mut bad_part = invalid.parts[0].clone();
            bad_part.mesh.indices.push(0);
            invalid.parts.push(bad_part);
            assert!(RetainedGltfModel::from_model(invalid, false).is_err());
            assert_eq!(crate::get_context().gl.retained_buffer_uploads, 0);

            for _ in 0..2 {
                source.draw();
                crate::get_context().perform_render_passes();
                crate::window::next_frame().await;
            }
            let immediate_updates = crate::get_context().gl.stream_buffer_updates;
            assert_eq!(immediate_updates, 4);

            let retained = RetainedGltfModel::from_model(source, false).unwrap();
            let instance = retained.clone();
            assert!(retained.cpu_model().is_none());
            assert!(Arc::ptr_eq(
                &retained.parts[0].mesh.buffers,
                &instance.parts[0].mesh.buffers
            ));
            assert_eq!(
                instance.parts[0]
                    .mesh
                    .texture
                    .as_ref()
                    .unwrap()
                    .raw_miniquad_id(),
                texture
            );
            for _ in 0..2 {
                retained.draw(Mat4::IDENTITY);
                instance.draw(Mat4::from_translation(Vec3::X));
                crate::get_context().perform_render_passes();
                crate::window::next_frame().await;
            }
            assert_eq!(crate::get_context().gl.retained_buffer_uploads, chunks * 2);
            assert_eq!(
                crate::get_context().gl.stream_buffer_updates,
                immediate_updates
            );
            println!(
                "glTF fixture: {chunks} chunks, {immediate_updates} immediate buffer updates over two frames; {} retained buffers uploaded once, zero additional geometry uploads for two instances over two frames",
                chunks * 2,
            );
            instance.draw(Mat4::IDENTITY);
            drop(retained);
            drop(instance);
            assert_eq!(crate::get_context().gl.retained_meshes.len(), chunks);
            crate::get_context().perform_render_passes();
            assert!(crate::get_context().gl.retained_meshes.is_empty());
            assert_eq!(
                crate::get_context().gl.retained_buffer_deletions,
                chunks * 2
            );
        });
    }

    // Kept separate from headless tests: this exercises actual native buffer
    // creation and drawing, with counters compiled out of normal builds.
    #[test]
    #[ignore = "requires a native graphics context"]
    fn retained_uploads_ordering_and_lifetime() {
        crate::Window::new("retained mesh buffer test", async {
            let source = Mesh {
                vertices: vec![Vertex::new(0., 0., 0., 0., 0., crate::color::WHITE); 3],
                indices: vec![0, 1, 2],
                texture: None,
            };
            let retained = RetainedMesh::from_mesh(&source).unwrap();
            let clone = retained.clone();
            let base = glam::Mat4::from_translation(glam::vec3(4., 5., 6.));
            let instance = glam::Mat4::from_scale(glam::vec3(2., 3., 4.));
            crate::get_context().gl.push_model_matrix(base);
            draw_retained_mesh(&retained, instance);
            draw_retained_mesh(&clone, glam::Mat4::IDENTITY);
            crate::get_context().gl.pop_model_matrix();
            {
                let gl = &crate::get_context().gl;
                assert_eq!(gl.retained_buffer_uploads, 2);
                assert_eq!(gl.draw_calls_count, 2);
                assert_eq!(gl.draw_calls[0].model, base * instance);
                assert_eq!(gl.draw_calls[1].model, base);
                assert!(gl.batch_vertex_buffer.is_empty());
                assert!(gl.batch_index_buffer.is_empty());
            }
            crate::get_context().perform_render_passes();
            crate::window::next_frame().await;
            draw_retained_mesh(&retained, glam::Mat4::IDENTITY);
            crate::get_context().perform_render_passes();
            {
                let gl = &crate::get_context().gl;
                assert_eq!(gl.retained_buffer_uploads, 2);
                assert_eq!(gl.stream_buffer_updates, 0);
                assert_eq!(gl.draw_calls_bindings.len(), 0);
                assert_eq!(gl.retained_meshes.len(), 1);
            }

            draw_mesh(&source);
            draw_retained_mesh(&clone, glam::Mat4::IDENTITY);
            draw_mesh(&source);
            {
                let gl = &crate::get_context().gl;
                assert_eq!(gl.draw_calls_count, 3);
                assert!(gl.draw_calls[0].retained.is_none());
                assert!(gl.draw_calls[1].retained.is_some());
                assert!(gl.draw_calls[2].retained.is_none());
                assert_eq!(gl.batch_vertex_buffer.len(), 6);
                assert_eq!(gl.batch_index_buffer, [0, 1, 2, 0, 1, 2]);
            }
            drop(retained);
            drop(clone);
            assert_eq!(crate::get_context().gl.retained_meshes.len(), 1);
            crate::get_context().perform_render_passes();
            {
                let gl = &crate::get_context().gl;
                assert_eq!(gl.retained_buffer_uploads, 2);
                assert_eq!(gl.stream_buffer_updates, 4);
                assert_eq!(gl.draw_calls_bindings.len(), 2);
                assert!(gl.retained_meshes.is_empty());
                assert_eq!(gl.retained_buffer_deletions, 2);
            }

            for reset in [false, true] {
                let mesh = RetainedMesh::from_mesh(&source).unwrap();
                draw_retained_mesh(&mesh, glam::Mat4::IDENTITY);
                drop(mesh);
                let gl = &mut crate::get_context().gl;
                if reset {
                    gl.reset();
                } else {
                    gl.clear_draw_calls();
                }
                assert!(gl.draw_calls.iter().all(|dc| dc.retained.is_none()));
                crate::get_context().perform_render_passes();
                assert!(crate::get_context().gl.retained_meshes.is_empty());
            }
            assert_eq!(crate::get_context().gl.retained_buffer_deletions, 6);

            let discarded = RetainedMesh::from_mesh(&source).unwrap();
            drop(discarded);
            let next = RetainedMesh::from_mesh(&source).unwrap();
            assert_eq!(crate::get_context().gl.retained_buffer_deletions, 8);
            assert_eq!(crate::get_context().gl.retained_meshes.len(), 1);
            drop(next);
            crate::get_context().perform_render_passes();
            assert_eq!(crate::get_context().gl.retained_buffer_deletions, 10);
        });
    }
}

mod shader {
    use miniquad::{ShaderMeta, UniformBlockLayout, UniformDesc, UniformType};

    pub const VERTEX: &str = r#"#version 100
    attribute vec3 position;
    attribute vec2 texcoord;
    attribute vec4 color0;
    attribute vec4 normal;

    varying lowp vec2 uv;
    varying lowp vec4 color;

    uniform mat4 Model;
    uniform mat4 Projection;

    void main() {
        gl_Position = Projection * Model * vec4(position, 1);
        color = color0 / 255.0;
        uv = texcoord;
    }"#;

    pub const FRAGMENT: &str = r#"#version 100
    varying lowp vec4 color;
    varying lowp vec2 uv;

    uniform sampler2D Texture;

    void main() {
        gl_FragColor = color * texture2D(Texture, uv) ;
    }"#;

    pub const METAL: &str = r#"
#include <metal_stdlib>
    using namespace metal;

    // Match shader::uniforms(): Metal reads the uploaded buffer positionally.
    struct Uniforms
    {
        float4x4 Projection;
        float4x4 Model;
        float4 _Time;
    };

    struct Vertex
    {
        float3 position    [[attribute(0)]];
        float2 texcoord    [[attribute(1)]];
        float4 color0      [[attribute(2)]];
    };

    struct RasterizerData
    {
        float4 position [[position]];
        float4 color [[user(locn0)]];
        float2 uv [[user(locn1)]];
    };

    vertex RasterizerData vertexShader(Vertex v [[stage_in]], constant Uniforms& uniforms [[buffer(0)]])
    {
        RasterizerData out;

        out.position = uniforms.Projection * uniforms.Model * float4(v.position, 1);
        out.color = v.color0 / 255.0;
        out.uv = v.texcoord;

        return out;
    }

    fragment float4 fragmentShader(RasterizerData in [[stage_in]], texture2d<float> tex [[texture(0)]], sampler texSmplr [[sampler(0)]])
    {
        return in.color * tex.sample(texSmplr, in.uv);
    }
    "#;
    pub fn uniforms() -> Vec<(&'static str, UniformType)> {
        vec![
            ("Projection", UniformType::Mat4),
            ("Model", UniformType::Mat4),
            ("_Time", UniformType::Float4),
        ]
    }

    pub fn meta() -> ShaderMeta {
        ShaderMeta {
            images: vec!["Texture".to_string(), "_ScreenTexture".to_string()],
            uniforms: UniformBlockLayout {
                uniforms: uniforms()
                    .into_iter()
                    .map(|(name, kind)| UniformDesc::new(name, kind))
                    .collect(),
            },
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use glam::{vec3, vec4, Mat4};

        fn builtin_fields(source: &str) -> Vec<(&str, &str)> {
            let declaration = source
                .split_once("struct Uniforms")
                .unwrap()
                .1
                .split_once('{')
                .unwrap()
                .1
                .split_once('}')
                .unwrap()
                .0;
            declaration
                .split(';')
                .take(3)
                .filter_map(|field| {
                    let mut words = field.split_whitespace();
                    let ty = words.next()?;
                    let name = words.next().unwrap();
                    assert!(words.next().is_none());
                    Some((name, ty))
                })
                .collect()
        }

        #[test]
        fn metal_builtins_match_uploaded_bytes_and_noncommuting_transforms() {
            // Metal consumes this buffer positionally; unlike GLSL, field
            // names cannot repair a different declaration order. Parse the
            // actual MSL declaration rather than repeating its expected order.
            let fields = builtin_fields(METAL);
            let meta = meta();
            assert_eq!(fields.len(), meta.uniforms.uniforms.len());

            // A pixel-space projection and translated/scaled model do not
            // commute. Identity models would conceal the original MSL bug.
            let projection = Mat4::from_cols_array(&[
                1. / 32.,
                0.,
                0.,
                0.,
                0.,
                1. / 32.,
                0.,
                0.,
                0.,
                0.,
                1.,
                0.,
                -1.,
                -1.,
                0.,
                1.,
            ]);
            let model =
                Mat4::from_translation(vec3(16., 0., 0.)) * Mat4::from_scale(vec3(2., 1., 1.));
            let mut bytes = Vec::new();
            let mut msl_offsets = std::collections::BTreeMap::new();
            for ((msl_name, msl_type), uniform) in fields.iter().zip(&meta.uniforms.uniforms) {
                assert_eq!(*msl_name, uniform.name);
                assert_eq!(uniform.array_count, 1);
                let values = match uniform.name.as_str() {
                    "Projection" => projection.to_cols_array().to_vec(),
                    "Model" => model.to_cols_array().to_vec(),
                    "_Time" => vec![2., 3., 5., 7.],
                    name => panic!("unexpected built-in: {name}"),
                };
                assert_eq!(uniform.uniform_type.size(), values.len() * 4);
                assert_eq!(
                    *msl_type,
                    if values.len() == 16 {
                        "float4x4"
                    } else {
                        "float4"
                    }
                );
                msl_offsets.insert(*msl_name, bytes.len());
                bytes.extend(values.into_iter().flat_map(f32::to_ne_bytes));
            }
            assert_eq!(bytes.len(), 144);
            let matrix = |name| {
                let offset = msl_offsets[name];
                Mat4::from_cols_array(&std::array::from_fn(|i| {
                    f32::from_ne_bytes(
                        bytes[offset + i * 4..offset + i * 4 + 4]
                            .try_into()
                            .unwrap(),
                    )
                }))
            };
            let position = vec4(4., 16., 0., 1.);
            assert_eq!(
                matrix("Projection") * matrix("Model") * position,
                vec4(-0.25, -0.5, 0., 1.)
            );
            assert_ne!(model * projection * position, vec4(-0.25, -0.5, 0., 1.));
            assert!(METAL.contains("uniforms.Projection * uniforms.Model * float4(v.position, 1)"));
        }

        #[test]
        fn material_metal_documentation_and_examples_use_the_builtin_prefix() {
            let expected = builtin_fields(METAL);
            let documentation = include_str!("material.rs")
                .lines()
                .map(|line| line.trim_start_matches("///"))
                .collect::<Vec<_>>()
                .join("\n");
            assert_eq!(builtin_fields(&documentation), expected);
            let examples = include_str!("../examples/custom_material.rs");
            let declarations: Vec<_> = examples.match_indices("struct Uniforms").collect();
            assert_eq!(declarations.len(), 2);
            for (offset, _) in declarations {
                assert_eq!(builtin_fields(&examples[offset..]), expected);
            }
        }
    }
}
