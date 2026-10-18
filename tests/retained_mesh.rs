use macroquad::miniquad::{UniformDesc, UniformType};
use macroquad::prelude::*;

fn rectangle(width: f32, color: Color) -> Mesh {
    Mesh {
        vertices: vec![
            Vertex::new(0., 0., 0., 0., 0., color),
            Vertex::new(width, 0., 0., 1., 0., color),
            Vertex::new(width, 64., 0., 1., 1., color),
            Vertex::new(0., 64., 0., 0., 1., color),
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
        texture: None,
    }
}

fn read_target(target: &RenderTarget) -> Image {
    unsafe { get_internal_gl().flush() };
    target.texture.get_texture_data()
}

fn assert_pixel(image: &Image, x: usize, expected: [u8; 4]) {
    // Read the middle row so render-target vertical orientation is irrelevant.
    let offset = (32 * image.width as usize + x) * 4;
    let actual = &image.bytes[offset..offset + 4];
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(actual, expected)| actual.abs_diff(expected) <= 2),
        "pixel at x={x}: expected {expected:?}, got {actual:?}"
    );
}

// One windowed test per binary: miniquad keeps a process-wide native display.
#[macroquad::test]
async fn retained_draws_preserve_order_transforms_material_state_and_resource_lifetime() {
    let target = render_target(64, 64);
    set_camera(&Camera2D {
        render_target: Some(target.clone()),
        ..Camera2D::from_display_rect(Rect::new(0., 0., 64., 64.))
    });

    let mut invalid = rectangle(32., WHITE);
    invalid.indices.clear();
    assert!(RetainedMesh::from_mesh(&invalid).is_err());
    invalid = rectangle(32., WHITE);
    invalid.vertices.clear();
    assert!(RetainedMesh::from_mesh(&invalid).is_err());
    invalid = rectangle(32., WHITE);
    invalid.indices[0] = 4;
    assert!(RetainedMesh::from_mesh(&invalid).is_err());
    invalid = rectangle(32., WHITE);
    invalid.indices.pop();
    assert!(RetainedMesh::from_mesh(&invalid).is_err());
    invalid = rectangle(32., WHITE);
    invalid.vertices.resize(65_537, invalid.vertices[0]);
    assert!(RetainedMesh::from_mesh(&invalid).is_err());
    drop(invalid);

    // A retained draw must stay between the immediate batches around it.
    // These paths share a pipeline and texture, so those cannot hide a merge.
    let middle = RetainedMesh::from_mesh(&rectangle(32., GREEN)).unwrap();
    clear_background(BLACK);
    draw_rectangle(0., 0., 64., 64., RED);
    draw_retained_mesh(&middle, Mat4::from_translation(vec3(16., 0., 0.)));
    draw_rectangle(24., 0., 16., 64., BLUE);
    let image = read_target(&target);
    assert_pixel(&image, 8, [230, 41, 55, 255]);
    assert_pixel(&image, 20, [0, 228, 48, 255]);
    assert_pixel(&image, 32, [0, 121, 241, 255]);
    assert_pixel(&image, 44, [0, 228, 48, 255]);
    assert_pixel(&image, 56, [230, 41, 55, 255]);

    // The supplied transform composes with the current model stack, and does
    // not remain on the stack for the following immediate draw.
    let shifted = RetainedMesh::from_mesh(&rectangle(16., RED)).unwrap();
    clear_background(BLACK);
    unsafe {
        get_internal_gl()
            .quad_gl
            .push_model_matrix(Mat4::from_translation(vec3(8., 0., 0.)));
    }
    draw_retained_mesh(&shifted, Mat4::from_translation(vec3(16., 0., 0.)));
    unsafe { get_internal_gl().quad_gl.pop_model_matrix() };
    draw_rectangle(48., 0., 16., 64., GREEN);
    let image = read_target(&target);
    assert_pixel(&image, 8, [0, 0, 0, 255]);
    assert_pixel(&image, 32, [230, 41, 55, 255]);
    assert_pixel(&image, 56, [0, 228, 48, 255]);

    // Each queued call retains its scissor, even when it changes twice before
    // flushing. Both source rectangles cover the full target without clipping.
    let clipped_red = RetainedMesh::from_mesh(&rectangle(64., RED)).unwrap();
    let clipped_green = RetainedMesh::from_mesh(&rectangle(64., GREEN)).unwrap();
    clear_background(BLACK);
    unsafe { get_internal_gl().quad_gl.scissor(Some((0, 0, 16, 64))) };
    draw_retained_mesh(&clipped_red, Mat4::IDENTITY);
    unsafe { get_internal_gl().quad_gl.scissor(Some((32, 0, 16, 64))) };
    draw_retained_mesh(&clipped_green, Mat4::IDENTITY);
    unsafe { get_internal_gl().quad_gl.scissor(None) };
    let image = read_target(&target);
    assert_pixel(&image, 8, [230, 41, 55, 255]);
    assert_pixel(&image, 24, [0, 0, 0, 255]);
    assert_pixel(&image, 40, [0, 228, 48, 255]);
    assert_pixel(&image, 56, [0, 0, 0, 255]);
    drop(clipped_red);
    drop(clipped_green);

    // Clones share geometry but may use different base textures. Both handles
    // and their source textures can be dropped while their draws are queued.
    let mut left = RetainedMesh::from_mesh(&rectangle(32., WHITE)).unwrap();
    left.texture = Some(Texture2D::from_rgba8(1, 1, &[255, 0, 0, 255]));
    let mut right = left.clone();
    right.texture = Some(Texture2D::from_rgba8(1, 1, &[0, 0, 255, 255]));
    clear_background(BLACK);
    draw_retained_mesh(&left, Mat4::IDENTITY);
    draw_retained_mesh(&right, Mat4::from_translation(vec3(32., 0., 0.)));
    drop(left);
    drop(right);
    let image = read_target(&target);
    assert_pixel(&image, 16, [255, 0, 0, 255]);
    assert_pixel(&image, 48, [0, 0, 255, 255]);

    let material = load_material(
        ShaderSource::Glsl {
            vertex: r#"#version 100
                attribute vec3 position;
                attribute vec2 texcoord;
                uniform mat4 Model;
                uniform mat4 Projection;
                varying lowp vec2 uv;
                void main() {
                    gl_Position = Projection * Model * vec4(position, 1.0);
                    uv = texcoord;
                }"#,
            fragment: r#"#version 100
                precision lowp float;
                varying lowp vec2 uv;
                uniform sampler2D CustomTexture;
                uniform vec4 Tint;
                void main() { gl_FragColor = texture2D(CustomTexture, uv) * Tint; }"#,
        },
        MaterialParams {
            uniforms: vec![UniformDesc::new("Tint", UniformType::Float4)],
            textures: vec!["CustomTexture".to_owned()],
            ..Default::default()
        },
    )
    .unwrap();
    let shared = RetainedMesh::from_mesh(&rectangle(32., WHITE)).unwrap();
    clear_background(BLACK);
    gl_use_material(&material);
    material.set_uniform("Tint", vec4(1., 0.5, 1., 1.));
    material.set_texture(
        "CustomTexture",
        Texture2D::from_rgba8(1, 1, &[200, 100, 50, 255]),
    );
    draw_retained_mesh(&shared, Mat4::IDENTITY);
    material.set_uniform("Tint", vec4(0.5, 1., 0.25, 1.));
    material.set_texture(
        "CustomTexture",
        Texture2D::from_rgba8(1, 1, &[20, 160, 240, 255]),
    );
    draw_retained_mesh(&shared, Mat4::from_translation(vec3(32., 0., 0.)));
    gl_use_default_material();
    drop(shared);
    let image = read_target(&target);
    assert_pixel(&image, 16, [200, 50, 50, 255]);
    assert_pixel(&image, 48, [10, 160, 60, 255]);

    // Clearing cancels queued geometry and retires its last handle safely.
    let cancelled = RetainedMesh::from_mesh(&rectangle(64., RED)).unwrap();
    draw_retained_mesh(&cancelled, Mat4::IDENTITY);
    drop(cancelled);
    clear_background(GREEN);
    draw_rectangle(24., 0., 16., 64., BLUE);
    let image = read_target(&target);
    assert_pixel(&image, 16, [0, 228, 48, 255]);
    assert_pixel(&image, 32, [0, 121, 241, 255]);

    // A retained quad exceeds both immediate capacities. Sample both of its
    // triangles to catch accidental clamping as well as buffer invalidation.
    macroquad::window::gl_set_drawcall_buffer_capacity(3, 3);
    clear_background(BLACK);
    draw_retained_mesh(&middle, Mat4::from_translation(vec3(16., 0., 0.)));
    let image = read_target(&target);
    assert_pixel(&image, 20, [0, 228, 48, 255]);
    assert_pixel(&image, 44, [0, 228, 48, 255]);
    macroquad::window::gl_set_drawcall_buffer_capacity(10_000, 5_000);
    drop(middle);
    drop(shifted);
    drop(material);
    set_default_camera();
    next_frame().await;
}
