use macroquad::{prelude::*, telemetry};

fn rectangle(texture: Texture2D) -> Mesh {
    Mesh {
        vertices: vec![
            Vertex::new(0., 0., 0., 0., 0., WHITE),
            Vertex::new(32., 0., 0., 1., 0., WHITE),
            Vertex::new(32., 64., 0., 1., 1., WHITE),
            Vertex::new(0., 64., 0., 0., 1., WHITE),
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
        texture: Some(texture),
    }
}

fn material() -> Material {
    load_material(
        ShaderSource::Glsl {
            vertex: r#"#version 100
                attribute vec3 position;
                uniform mat4 Model;
                uniform mat4 Projection;
                void main() { gl_Position = Projection * Model * vec4(position, 1.0); }"#,
            fragment: r#"#version 100
                precision lowp float;
                uniform sampler2D CustomTexture;
                void main() { gl_FragColor = texture2D(CustomTexture, vec2(0.5)); }"#,
        },
        MaterialParams {
            textures: vec!["CustomTexture".to_owned()],
            ..Default::default()
        },
    )
    .unwrap()
}

#[track_caller]
fn assert_pixel(image: &Image, x: usize, expected: [u8; 4]) {
    let offset = (32 * image.width as usize + x) * 4;
    assert_eq!(&image.bytes[offset..offset + 4], &expected);
}

fn assert_trimmed() {
    let report = telemetry::renderer_diagnostics();
    assert_eq!(report.queued_draw_calls, 0);
    assert_eq!(report.cpu_vertex_bytes, 0);
    assert_eq!(report.cpu_vertex_capacity_bytes, 0);
    assert_eq!(report.cpu_index_bytes, 0);
    assert_eq!(report.cpu_index_capacity_bytes, 0);
    assert_eq!(report.streaming_gpu_buffer_count, 0);
    assert_eq!(report.estimated_streaming_gpu_buffer_capacity_bytes, 0);
}

fn assert_baseline(textures: usize) {
    assert_trimmed();
    let report = telemetry::renderer_diagnostics();
    assert_eq!(report.retained_mesh_count, 0);
    assert_eq!(report.retained_gpu_buffer_count, 0);
    assert_eq!(report.estimated_retained_gpu_buffer_bytes, 0);
    assert_eq!(telemetry::textures_count(), textures);
}

// One windowed test per binary because miniquad owns a global display.
#[macroquad::test]
async fn retirement_submits_pending_draws_preserves_live_handles_and_trims_scene_caches() {
    let target = render_target(64, 64);
    let camera = Camera2D {
        render_target: Some(target.clone()),
        ..Camera2D::from_display_rect(Rect::new(0., 0., 64., 64.))
    };
    next_frame().await;
    set_camera(&camera);
    // Each quad fits, but two cannot batch. Retirement must preserve these
    // caller-configured limits instead of substituting library defaults.
    macroquad::window::gl_set_drawcall_buffer_capacity(5, 7);
    telemetry::set_renderer_diagnostics_enabled(true);
    retire_unused_render_resources();
    let baseline_textures = telemetry::textures_count();
    let white_id = Texture2D::empty().raw_miniquad_id();
    assert_baseline(baseline_textures);

    // Both immediate base textures and retained meshes can lose their public
    // owners before retirement. Submission must precede every deletion.
    clear_background(BLACK);
    let immediate = rectangle(Texture2D::from_rgba8(1, 1, &[10, 20, 30, 255]));
    let immediate_weak = immediate.texture.as_ref().unwrap().weak_clone();
    let source = rectangle(Texture2D::from_rgba8(1, 1, &[200, 100, 50, 255]));
    let retained_weak = source.texture.as_ref().unwrap().weak_clone();
    let retained = RetainedMesh::from_mesh(&source).unwrap();
    draw_mesh(&immediate);
    draw_retained_mesh(&retained, Mat4::from_translation(vec3(32., 0., 0.)));
    drop(immediate);
    drop(source);
    drop(retained);
    assert_eq!(telemetry::renderer_diagnostics().queued_draw_calls, 2);
    telemetry::reset_renderer_diagnostics();
    retire_unused_render_resources();
    let pixels = target.texture.get_texture_data();
    assert_pixel(&pixels, 16, [10, 20, 30, 255]);
    assert_pixel(&pixels, 48, [200, 100, 50, 255]);
    let counters = telemetry::renderer_diagnostics().counters;
    assert_eq!(counters.immediate_draw_calls, 1);
    assert_eq!(counters.retained_draw_calls, 1);
    assert_eq!(counters.streaming_buffer_uploads, 2);
    assert_eq!(immediate_weak.raw_miniquad_id(), white_id);
    assert_eq!(retained_weak.raw_miniquad_id(), white_id);
    assert_baseline(baseline_textures);

    // Surviving shared mesh/texture handles, the active render target and a
    // custom material's sole texture owner all remain usable after trimming.
    let source = rectangle(Texture2D::from_rgba8(1, 1, &[40, 90, 140, 255]));
    let live_texture = source.texture.as_ref().unwrap().weak_clone();
    let live_texture_id = live_texture.raw_miniquad_id();
    let retained = RetainedMesh::from_mesh(&source).unwrap();
    let shared = retained.clone();
    drop(source);
    drop(retained);
    let material = material();
    let custom = Texture2D::from_rgba8(1, 1, &[70, 130, 190, 255]);
    assert_eq!(live_texture.get_texture_data().bytes, [40, 90, 140, 255]);
    let custom_weak = custom.weak_clone();
    let custom_id = custom.raw_miniquad_id();
    material.set_texture("CustomTexture", custom);
    retire_unused_render_resources();
    assert_trimmed();
    assert_eq!(telemetry::renderer_diagnostics().retained_mesh_count, 1);

    // An ordinary pause advances frames while keeping the scene owned. It
    // must not discard or recreate its retained resources.
    for _ in 0..2 {
        next_frame().await;
        assert_eq!(telemetry::renderer_diagnostics().retained_mesh_count, 1);
        assert_eq!(live_texture.raw_miniquad_id(), live_texture_id);
        assert_eq!(custom_weak.raw_miniquad_id(), custom_id);
    }
    set_camera(&camera);
    clear_background(BLACK);
    telemetry::reset_renderer_diagnostics();
    draw_retained_mesh(&shared, Mat4::IDENTITY);
    gl_use_material(&material);
    draw_rectangle(32., 0., 32., 64., WHITE);
    retire_unused_render_resources();
    let pixels = target.texture.get_texture_data();
    assert_pixel(&pixels, 16, [40, 90, 140, 255]);
    assert_pixel(&pixels, 48, [70, 130, 190, 255]);
    // Retirement preserves the selected material as well as the camera.
    draw_rectangle(32., 0., 32., 64., WHITE);
    retire_unused_render_resources();
    let pixels = target.texture.get_texture_data();
    assert_pixel(&pixels, 16, [40, 90, 140, 255]);
    assert_pixel(&pixels, 48, [70, 130, 190, 255]);
    assert_eq!(
        telemetry::renderer_diagnostics()
            .counters
            .retained_buffer_uploads,
        0
    );
    gl_use_default_material();
    drop(shared);
    drop(material);
    retire_unused_render_resources();
    assert_eq!(live_texture.raw_miniquad_id(), white_id);
    assert_eq!(custom_weak.raw_miniquad_id(), white_id);
    assert_baseline(baseline_textures);

    // Alternate scenes with different texture dimensions and stream-slot
    // counts. Their temporary allocations must return to the same baseline.
    for cycle in 0..8 {
        let (size, draws, color) = if cycle % 2 == 0 {
            (1, 2, [12, 34, 56, 255])
        } else {
            (2, 9, [210, 150, 90, 255])
        };
        clear_background(BLACK);
        let pixels = color.repeat(size * size);
        let source = rectangle(Texture2D::from_rgba8(size as u16, size as u16, &pixels));
        let weak = source.texture.as_ref().unwrap().weak_clone();
        let retained = RetainedMesh::from_mesh(&source).unwrap();
        for _ in 0..draws {
            draw_mesh(&source);
        }
        draw_retained_mesh(&retained, Mat4::from_translation(vec3(32., 0., 0.)));
        assert_eq!(
            telemetry::renderer_diagnostics().queued_draw_calls,
            draws + 1
        );
        // Ordinary submission retains its stream caches for reuse.
        unsafe { get_internal_gl().flush() };
        let cached = telemetry::renderer_diagnostics();
        assert_eq!(cached.streaming_gpu_buffer_count, draws * 2);
        assert!(cached.cpu_vertex_capacity_bytes > 0);
        assert!(cached.cpu_index_capacity_bytes > 0);
        // Queue the same scene again, then release it without a manual flush.
        draw_mesh(&source);
        draw_retained_mesh(&retained, Mat4::from_translation(vec3(32., 0., 0.)));
        drop(source);
        drop(retained);
        retire_unused_render_resources();
        let pixels = target.texture.get_texture_data();
        assert_pixel(&pixels, 16, color);
        assert_pixel(&pixels, 48, color);
        assert_eq!(weak.raw_miniquad_id(), white_id);
        assert_baseline(baseline_textures);
        retire_unused_render_resources();
        assert_baseline(baseline_textures);
    }
    telemetry::set_renderer_diagnostics_enabled(false);
    set_default_camera();
}
