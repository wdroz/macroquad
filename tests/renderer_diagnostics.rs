use macroquad::prelude::*;
use macroquad::telemetry::{self, RendererCounters};

fn triangle() -> Mesh {
    Mesh {
        vertices: vec![
            Vertex::new(0., 0., 0., 0., 0., WHITE),
            Vertex::new(16., 0., 0., 0., 0., WHITE),
            Vertex::new(0., 16., 0., 0., 0., WHITE),
        ],
        indices: vec![0, 1, 2],
        texture: None,
    }
}

fn flush() {
    unsafe { get_internal_gl().flush() };
}

// One native window per test binary. The public report must follow actual
// uploads, submission, cancellation, sharing and collection boundaries.
#[macroquad::test]
async fn renderer_reports_resources_and_opt_in_submitted_work() {
    let source = triangle();
    let vertex_bytes = source.vertices.len() * std::mem::size_of::<Vertex>();
    let index_bytes = source.indices.len() * std::mem::size_of::<u16>();
    let geometry_bytes = vertex_bytes + index_bytes;
    let initial = telemetry::renderer_diagnostics();
    assert!(!initial.counters_enabled);
    assert_eq!(initial.counters, RendererCounters::default());
    assert_eq!(initial.queued_draw_calls, 0);
    assert_eq!(initial.retained_mesh_count, 0);
    assert_eq!(initial.streaming_gpu_buffer_count, 0);

    // Disabled instrumentation still exposes live resource sizes.
    let unmeasured = RetainedMesh::from_mesh(&source).unwrap();
    draw_retained_mesh(&unmeasured, Mat4::IDENTITY);
    flush();
    let disabled = telemetry::renderer_diagnostics();
    assert_eq!(disabled.retained_mesh_count, 1);
    assert_eq!(disabled.retained_gpu_buffer_count, 2);
    assert_eq!(disabled.estimated_retained_gpu_buffer_bytes, geometry_bytes);
    assert_eq!(disabled.counters, RendererCounters::default());
    drop(unmeasured);
    flush();

    telemetry::set_renderer_diagnostics_enabled(true);
    let mesh = RetainedMesh::from_mesh(&source).unwrap();
    let instance = mesh.clone();
    let uploaded = telemetry::renderer_diagnostics();
    assert!(uploaded.counters_enabled);
    assert_eq!(uploaded.counters.retained_buffer_uploads, 2);
    assert_eq!(
        uploaded.counters.retained_buffer_upload_bytes,
        geometry_bytes as u64
    );
    assert_eq!(uploaded.retained_mesh_count, 1);
    assert_eq!(uploaded.estimated_retained_gpu_buffer_bytes, geometry_bytes);

    // Adjacent immediate submissions batch into one draw/update pair. Retained
    // instances stay separate and do not append CPU geometry or upload it again.
    draw_mesh(&source);
    draw_mesh(&source);
    draw_retained_mesh(&mesh, Mat4::IDENTITY);
    draw_retained_mesh(&instance, Mat4::from_translation(Vec3::X));
    let queued = telemetry::renderer_diagnostics();
    assert_eq!(queued.queued_draw_calls, 3);
    assert_eq!(queued.cpu_vertex_bytes, vertex_bytes * 2);
    assert_eq!(queued.cpu_index_bytes, index_bytes * 2);
    assert!(queued.cpu_vertex_capacity_bytes >= queued.cpu_vertex_bytes);
    assert!(queued.cpu_index_capacity_bytes >= queued.cpu_index_bytes);
    assert_eq!(queued.counters.draw_calls, 0);
    assert_eq!(queued.streaming_gpu_buffer_count, 0);
    assert_eq!(queued, telemetry::renderer_diagnostics());

    // Dropping all public handles must keep queued resources visible and valid.
    drop(mesh);
    drop(instance);
    assert_eq!(telemetry::renderer_diagnostics().retained_mesh_count, 1);
    flush();
    let submitted = telemetry::renderer_diagnostics();
    assert_eq!(submitted.counters.draw_calls, 3);
    assert_eq!(submitted.counters.immediate_draw_calls, 1);
    assert_eq!(submitted.counters.retained_draw_calls, 2);
    assert_eq!(submitted.counters.auxiliary_draw_calls, 0);
    assert_eq!(submitted.counters.streaming_buffer_uploads, 2);
    assert_eq!(
        submitted.counters.streaming_buffer_upload_bytes,
        geometry_bytes as u64 * 2
    );
    assert_eq!(submitted.counters.retained_buffer_uploads, 2);
    assert_eq!(
        submitted.counters.retained_buffer_upload_bytes,
        geometry_bytes as u64
    );
    assert_eq!(submitted.queued_draw_calls, 0);
    assert_eq!(submitted.cpu_vertex_bytes, 0);
    assert_eq!(submitted.cpu_index_bytes, 0);
    assert_eq!(
        submitted.cpu_vertex_capacity_bytes,
        queued.cpu_vertex_capacity_bytes
    );
    assert_eq!(
        submitted.cpu_index_capacity_bytes,
        queued.cpu_index_capacity_bytes
    );
    assert_eq!(submitted.retained_gpu_buffer_count, 0);
    assert_eq!(submitted.estimated_retained_gpu_buffer_bytes, 0);
    assert_eq!(submitted.streaming_gpu_buffer_count, 2);
    assert_eq!(
        submitted.estimated_streaming_gpu_buffer_capacity_bytes,
        geometry_bytes * 2
    );

    // A smaller batch reuses capacity but counts only the bytes uploaded now.
    telemetry::reset_renderer_diagnostics();
    draw_mesh(&source);
    flush();
    let reused = telemetry::renderer_diagnostics();
    assert_eq!(
        reused.counters.streaming_buffer_upload_bytes,
        geometry_bytes as u64
    );
    assert_eq!(
        reused.estimated_streaming_gpu_buffer_capacity_bytes,
        geometry_bytes * 2
    );

    // Reset is independent of pending work, resource ownership and enablement.
    let mesh = RetainedMesh::from_mesh(&source).unwrap();
    draw_retained_mesh(&mesh, Mat4::IDENTITY);
    telemetry::reset_renderer_diagnostics();
    assert_eq!(telemetry::renderer_diagnostics().queued_draw_calls, 1);
    assert_eq!(
        telemetry::renderer_diagnostics().counters,
        RendererCounters::default()
    );
    telemetry::set_renderer_diagnostics_enabled(false);
    flush();
    assert_eq!(
        telemetry::renderer_diagnostics().counters,
        RendererCounters::default()
    );
    telemetry::set_renderer_diagnostics_enabled(true);
    draw_retained_mesh(&mesh, Mat4::IDENTITY);
    clear_background(BLACK);
    flush();
    assert_eq!(telemetry::renderer_diagnostics().counters.draw_calls, 0);
    drop(mesh);
    flush();

    // Repeated uploads/queued drops return to the same GPU baseline, without
    // mistaking a surviving high-water streaming allocation for a retained leak.
    telemetry::reset_renderer_diagnostics();
    for cycle in 1..=4 {
        let mesh = RetainedMesh::from_mesh(&source).unwrap();
        let shared = mesh.clone();
        draw_retained_mesh(&shared, Mat4::IDENTITY);
        drop(mesh);
        drop(shared);
        let pending = telemetry::renderer_diagnostics();
        assert_eq!(pending.retained_mesh_count, 1);
        assert_eq!(pending.estimated_retained_gpu_buffer_bytes, geometry_bytes);
        flush();
        let collected = telemetry::renderer_diagnostics();
        assert_eq!(collected.retained_mesh_count, 0);
        assert_eq!(collected.estimated_retained_gpu_buffer_bytes, 0);
        assert_eq!(
            collected.estimated_streaming_gpu_buffer_capacity_bytes,
            geometry_bytes * 2
        );
        assert_eq!(collected.counters.retained_buffer_uploads, cycle * 2);
        assert_eq!(
            collected.counters.retained_buffer_upload_bytes,
            cycle * geometry_bytes as u64
        );
        assert_eq!(collected.counters.retained_draw_calls, cycle);
        assert_eq!(collected.counters.streaming_buffer_uploads, 0);
    }
    // Growing a reused stream slot reports the geometric capacity separately
    // from the exact initialized bytes uploaded for this batch.
    telemetry::reset_renderer_diagnostics();
    for _ in 0..3 {
        draw_mesh(&source);
    }
    flush();
    let grown = telemetry::renderer_diagnostics();
    assert_eq!(grown.counters.streaming_buffer_uploads, 2);
    assert_eq!(
        grown.counters.streaming_buffer_upload_bytes,
        geometry_bytes as u64 * 3
    );
    assert_eq!(
        grown.estimated_streaming_gpu_buffer_capacity_bytes,
        geometry_bytes * 4
    );

    // Auxiliary backend draws must not be mistaken for either geometry path.
    let target = render_target(32, 32);
    set_camera(&Camera2D {
        render_target: Some(target.clone()),
        ..Camera2D::from_display_rect(Rect::new(0., 0., 32., 32.))
    });
    let material = load_material(
        ShaderSource::Glsl {
            vertex: r#"#version 100
                attribute vec3 position;
                uniform mat4 Model;
                uniform mat4 Projection;
                void main() { gl_Position = Projection * Model * vec4(position, 1.0); }"#,
            fragment: r#"#version 100
                precision lowp float;
                uniform sampler2D _ScreenTexture;
                void main() { gl_FragColor = texture2D(_ScreenTexture, vec2(0.5)); }"#,
        },
        MaterialParams::default(),
    )
    .unwrap();
    telemetry::reset_renderer_diagnostics();
    gl_use_material(&material);
    draw_mesh(&source);
    flush();
    gl_use_default_material();
    let copied = telemetry::renderer_diagnostics();
    assert_eq!(copied.counters.draw_calls, 2);
    assert_eq!(copied.counters.immediate_draw_calls, 1);
    assert_eq!(copied.counters.auxiliary_draw_calls, 1);

    telemetry::capture_frame();
    next_frame().await;
    telemetry::reset_renderer_diagnostics();
    draw_mesh(&source);
    flush();
    let captured = telemetry::renderer_diagnostics();
    assert_eq!(captured.counters.draw_calls, 2);
    assert_eq!(captured.counters.immediate_draw_calls, 1);
    assert_eq!(captured.counters.auxiliary_draw_calls, 1);
    telemetry::set_renderer_diagnostics_enabled(false);
}
