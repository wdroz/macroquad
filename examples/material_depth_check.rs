//! GPU regression for transparent depth tests and queued custom samplers.
//! Run `cargo run --example material_depth_check`; exits after pixel assertions.
use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Equation};
use macroquad::prelude::*;

const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec4 color0;
varying lowp vec4 color;
void main() { gl_Position = vec4(position, 1.0); color = color0 / 255.0; }
"#;
const FRAGMENT: &str = r#"#version 100
precision mediump float;
varying lowp vec4 color;
uniform sampler2D Tint;
void main() { gl_FragColor = color * texture2D(Tint, vec2(0.5)); }
"#;

fn material(blend: bool) -> Material {
    load_material(
        ShaderSource::Glsl {
            vertex: VERTEX,
            fragment: FRAGMENT,
        },
        MaterialParams {
            pipeline_params: PipelineParams {
                depth_test: Comparison::LessOrEqual,
                depth_write: !blend,
                color_blend: blend.then(|| {
                    BlendState::new(
                        Equation::Add,
                        BlendFactor::Value(BlendValue::SourceAlpha),
                        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                    )
                }),
                ..Default::default()
            },
            textures: vec!["Tint".into()],
            ..Default::default()
        },
    )
    .unwrap()
}

fn quad(left: f32, right: f32, z: f32, color: Color) {
    draw_mesh(&Mesh {
        vertices: vec![
            Vertex::new(left, -1.0, z, 0.0, 0.0, color),
            Vertex::new(right, -1.0, z, 0.0, 0.0, color),
            Vertex::new(right, 1.0, z, 0.0, 0.0, color),
            Vertex::new(left, 1.0, z, 0.0, 0.0, color),
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
        texture: None,
    });
}

fn check(image: &Image, x: u32, expected: [u8; 3], label: &str) {
    let pixel = image.get_pixel(x, 16);
    let actual = [pixel.r, pixel.g, pixel.b].map(|v| (v * 255.0).round() as u8);
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(&a, b)| a.abs_diff(b) <= 2),
        "{label}: expected {expected:?}, got {actual:?}"
    );
}

#[macroquad::main("Material GPU checks")]
async fn main() {
    let target = render_target_ex(
        32,
        32,
        RenderTargetParams {
            sample_count: 1,
            depth: true,
        },
    );
    let opaque = material(false);
    let transparent = material(true);
    let white = Texture2D::from_rgba8(1, 1, &[255; 4]);
    opaque.set_texture("Tint", white.clone());
    transparent.set_texture("Tint", white);
    set_camera(&Camera2D {
        render_target: Some(target.clone()),
        ..Default::default()
    });
    clear_background(BLACK);
    gl_use_material(&opaque);
    quad(-1.0, 0.0, 0.0, BLUE);
    quad(0.0, 1.0, 0.8, BLUE);
    gl_use_material(&transparent);
    quad(-1.0, 0.0, 0.5, Color::new(1.0, 0.0, 0.0, 0.5));
    quad(0.0, 1.0, 0.0, Color::new(1.0, 0.0, 0.0, 0.5));
    quad(0.0, 1.0, 0.4, Color::new(0.0, 1.0, 0.0, 0.5));
    unsafe {
        get_internal_gl().flush();
    }
    let image = target.texture.get_texture_data();
    check(&image, 8, [0, 121, 241], "opaque occludes glass");
    check(
        &image,
        24,
        [64, 158, 61],
        "glass tests depth without writing depth",
    );

    // A clear after the final transparent batch must reset the depth buffer.
    clear_background(BLACK);
    gl_use_material(&opaque);
    let red = Texture2D::from_rgba8(1, 1, &[255, 0, 0, 255]);
    let green = Texture2D::from_rgba8(1, 1, &[0, 255, 0, 255]);
    opaque.set_texture("Tint", red.clone());
    quad(-1.0, 0.0, 0.9, WHITE);
    opaque.set_texture("Tint", green.clone());
    quad(0.0, 1.0, 0.9, WHITE);
    unsafe {
        get_internal_gl().flush();
    }
    let image = target.texture.get_texture_data();
    check(&image, 8, [255, 0, 0], "first queued sampler and clear");
    check(&image, 24, [0, 255, 0], "second queued sampler");

    // Reuse a slot first with more geometry, then less. Streaming buffers must
    // grow before upload and remain valid when that larger allocation is reused.
    for count in [20, 1] {
        clear_background(BLACK);
        for _ in 0..count {
            quad(-1.0, 1.0, 0.0, WHITE);
        }
        unsafe {
            get_internal_gl().flush();
        }
        check(
            &target.texture.get_texture_data(),
            16,
            [0, 255, 0],
            "grown/reused stream buffer",
        );
    }
    macroquad::window::gl_set_drawcall_buffer_capacity(128, 192);
    clear_background(BLACK);
    quad(-1.0, 1.0, 0.0, WHITE);
    unsafe {
        get_internal_gl().flush();
    }
    check(
        &target.texture.get_texture_data(),
        16,
        [0, 255, 0],
        "buffers rebuilt after capacity reset",
    );
    println!(
        "PASS: opaque occlusion, transparent depth mask, depth clear, queued texture bindings, adaptive buffer growth/reuse/reset"
    );
    gl_use_default_material();
    set_default_camera();
}
