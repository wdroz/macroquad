use macroquad::prelude::*;

fn material() -> Material {
    load_material(
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
                void main() { gl_FragColor = texture2D(CustomTexture, uv); }"#,
        },
        MaterialParams {
            textures: vec!["CustomTexture".to_owned()],
            ..Default::default()
        },
    )
    .unwrap()
}

fn read(target: &RenderTarget) -> Image {
    unsafe { get_internal_gl().flush() };
    target.texture.get_texture_data()
}

fn assert_pixel(image: &Image, x: usize, expected: [u8; 4]) {
    let offset = (32 * image.width as usize + x) * 4;
    assert_eq!(&image.bytes[offset..offset + 4], &expected);
}

// One windowed regression per binary because miniquad owns a global display.
#[macroquad::test]
async fn material_textures_survive_frames_and_release_completed_snapshots() {
    let target = render_target(64, 64);
    let camera = Camera2D {
        render_target: Some(target.clone()),
        ..Camera2D::from_display_rect(Rect::new(0., 0., 64., 64.))
    };
    let mesh = RetainedMesh::from_mesh(&Mesh {
        vertices: vec![
            Vertex::new(0., 0., 0., 0., 0., WHITE),
            Vertex::new(32., 0., 0., 1., 0., WHITE),
            Vertex::new(32., 64., 0., 1., 1., WHITE),
            Vertex::new(0., 64., 0., 0., 1., WHITE),
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
        texture: None,
    })
    .unwrap();
    let material = material();
    let texture = Texture2D::from_rgba8(1, 1, &[200, 100, 50, 255]);
    let original = texture.weak_clone();
    let original_id = texture.raw_miniquad_id();
    material.set_texture("CustomTexture", texture);

    // set_texture consumes the only strong handle. Both drawing paths must
    // keep sampling it after end-of-frame texture garbage collection.
    for frame in 0..2 {
        set_camera(&camera);
        clear_background(BLACK);
        gl_use_material(&material);
        draw_retained_mesh(&mesh, Mat4::IDENTITY);
        draw_rectangle(32., 0., 32., 64., WHITE);
        gl_use_default_material();
        let image = read(&target);
        assert_pixel(&image, 16, [200, 100, 50, 255]);
        assert_pixel(&image, 48, [200, 100, 50, 255]);
        println!("material frame {frame}: retained and immediate pixels verified");
        next_frame().await;
    }
    assert_eq!(original.raw_miniquad_id(), original_id);

    // Rebinding snapshots the previous texture for queued draws. Exercise
    // both retained and immediate snapshots and release their cached handles
    // after submission, even if these draw-call slots are never reused.
    set_camera(&camera);
    clear_background(BLACK);
    gl_use_material(&material);
    draw_retained_mesh(&mesh, Mat4::IDENTITY);
    let replacement = Texture2D::from_rgba8(1, 1, &[20, 160, 240, 255]);
    let replaced = replacement.weak_clone();
    material.set_texture("CustomTexture", replacement);
    draw_rectangle(32., 0., 32., 64., WHITE);
    let final_texture = Texture2D::from_rgba8(1, 1, &[64, 128, 192, 255]);
    let final_weak = final_texture.weak_clone();
    let final_id = final_texture.raw_miniquad_id();
    material.set_texture("CustomTexture", final_texture);
    gl_use_default_material();
    let image = read(&target);
    assert_pixel(&image, 16, [200, 100, 50, 255]);
    assert_pixel(&image, 48, [20, 160, 240, 255]);
    next_frame().await;
    let white_id = Texture2D::empty().raw_miniquad_id();
    assert_eq!(original.raw_miniquad_id(), white_id);
    assert_eq!(replaced.raw_miniquad_id(), white_id);
    assert_eq!(final_weak.raw_miniquad_id(), final_id);

    // Cancelling also releases both snapshot kinds. An unmanaged white
    // texture remains borrowed and does not create an owned GPU resource.
    set_camera(&camera);
    gl_use_material(&material);
    draw_retained_mesh(&mesh, Mat4::IDENTITY);
    draw_rectangle(32., 0., 32., 64., WHITE);
    material.set_texture("CustomTexture", Texture2D::empty());
    gl_use_default_material();
    clear_background(BLACK);
    next_frame().await;
    assert_eq!(final_weak.raw_miniquad_id(), white_id);

    // A deliberately weak binding stays weak: the caller must keep an owner
    // alive. After its owner disappears the regular white fallback applies.
    let owner = Texture2D::from_rgba8(1, 1, &[255, 0, 0, 255]);
    let weak = owner.weak_clone();
    material.set_texture("CustomTexture", weak.clone());
    drop(owner);
    next_frame().await;
    assert_eq!(weak.raw_miniquad_id(), white_id);
    set_camera(&camera);
    clear_background(BLACK);
    gl_use_material(&material);
    draw_retained_mesh(&mesh, Mat4::IDENTITY);
    gl_use_default_material();
    assert_pixel(&read(&target), 16, [255, 255, 255, 255]);

    // Dropping the material after its final flush releases the last binding.
    let last = Texture2D::from_rgba8(1, 1, &[100, 150, 200, 255]);
    let last_weak = last.weak_clone();
    material.set_texture("CustomTexture", last);
    gl_use_material(&material);
    draw_retained_mesh(&mesh, Mat4::IDENTITY);
    gl_use_default_material();
    assert_pixel(&read(&target), 16, [100, 150, 200, 255]);
    drop(material);
    next_frame().await;
    assert_eq!(last_weak.raw_miniquad_id(), white_id);
    set_default_camera();
}
