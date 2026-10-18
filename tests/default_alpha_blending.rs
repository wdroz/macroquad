use macroquad::prelude::*;

fn read_pixel(target: &RenderTarget) -> [u8; 4] {
    // Flush the actual default GPU pipeline before reading the attachment.
    unsafe { get_internal_gl().flush() };
    target.texture.get_texture_data().bytes[..4]
        .try_into()
        .unwrap()
}

fn close(actual: u8, expected: u8) {
    assert!(
        actual.abs_diff(expected) <= 2,
        "expected {expected}, got {actual}"
    );
}

// One windowed test per binary: miniquad keeps a process-wide native display.
#[macroquad::test]
async fn default_draws_preserve_opaque_alpha_and_accumulate_transparent_coverage() {
    let target = render_target(32, 32);
    let camera = Camera2D {
        render_target: Some(target.clone()),
        ..Camera2D::from_display_rect(Rect::new(0., 0., 32., 32.))
    };
    set_camera(&camera);
    clear_background(BLACK);
    draw_rectangle(0., 0., 32., 32., Color::new(1., 0., 0., 0.5));
    let pixel = read_pixel(&target);
    close(pixel[0], 128);
    assert_eq!(
        pixel[3], 255,
        "translucent shapes must not make opaque targets transparent"
    );

    // Antialiased glyph edges previously reduced the canvas alpha, making
    // browser text washed out when the browser composited it over the page.
    clear_background(WHITE);
    draw_text("Aa", 2., 24., 24., BLACK);
    unsafe { get_internal_gl().flush() };
    let image = target.texture.get_texture_data();
    assert!(image.bytes.chunks_exact(4).all(|rgba| rgba[3] == 255));
    assert!(
        image.bytes.chunks_exact(4).any(|rgba| rgba[0] < 200),
        "glyphs must have rendered"
    );

    // Source-over must also accumulate coverage on transparent attachments;
    // merely disabling alpha writes would pass the opaque checks above.
    clear_background(Color::new(0., 0., 0., 0.));
    draw_rectangle(0., 0., 32., 32., Color::new(1., 0., 0., 0.5));
    close(read_pixel(&target)[3], 128);
    draw_rectangle(0., 0., 32., 32., Color::new(0., 0., 1., 0.5));
    let pixel = read_pixel(&target);
    close(pixel[0], 64);
    close(pixel[2], 128);
    close(pixel[3], 192);

    // User materials retain the supplied pipeline. With blending disabled a
    // half-alpha source must replace, rather than composite over, the target.
    let material = load_material(
        ShaderSource::Glsl {
            vertex: r#"#version 100
                attribute vec3 position;
                attribute vec4 color0;
                uniform mat4 Model;
                uniform mat4 Projection;
                varying lowp vec4 color;
                void main() {
                    gl_Position = Projection * Model * vec4(position, 1.0);
                    color = color0 / 255.0;
                }"#,
            fragment: r#"#version 100
                varying lowp vec4 color;
                void main() { gl_FragColor = color; }"#,
        },
        MaterialParams::default(),
    )
    .unwrap();
    clear_background(WHITE);
    gl_use_material(&material);
    draw_rectangle(0., 0., 32., 32., Color::new(1., 0., 0., 0.5));
    let pixel = read_pixel(&target);
    assert_eq!(&pixel[..3], &[255, 0, 0]);
    close(pixel[3], 128);
    gl_use_default_material();

    // The immediate-mode root UI uses its own material and is submitted at
    // end_frame. Exercise its antialiased glyphs on a screen-sized attachment.
    let ui_target = render_target(screen_width() as u32, screen_height() as u32);
    set_camera(&Camera2D {
        render_target: Some(ui_target.clone()),
        ..Camera2D::from_display_rect(Rect::new(0., 0., screen_width(), screen_height()))
    });
    clear_background(WHITE);
    macroquad::ui::root_ui().label(Some(vec2(20., 20.)), "Antialiased UI label");
    next_frame().await;
    let image = ui_target.texture.get_texture_data();
    assert!(
        image.bytes.chunks_exact(4).all(|rgba| rgba[3] == 255),
        "root UI must preserve opaque target alpha"
    );
    assert!(
        image.bytes.chunks_exact(4).any(|rgba| rgba[0] < 200),
        "root UI glyphs must have rendered"
    );
    set_default_camera();
}
