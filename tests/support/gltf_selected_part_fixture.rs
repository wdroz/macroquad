//! Selected chunks use the same passes, transforms and borrowed resources as
//! whole models. Keep these regressions shared by native and WebGL checks.

use super::*;
use macroquad::telemetry;

fn submit_part<'a>(
    frame: &mut GltfMaterialFrame<'a>,
    geometry: Geometry,
    fixture: &'a Fixture,
    index: usize,
    transform: Mat4,
) -> Result<(), String> {
    match geometry {
        Geometry::Cpu => frame.submit_part(&fixture.cpu.parts[index], transform),
        Geometry::Retained => frame.submit_retained_part(&fixture.retained.parts[index], transform),
    }
}

pub(super) fn run(renderer: &mut GltfMaterialRenderer, target: &RenderTarget, camera: &Camera3D) {
    let identity = Mat4::IDENTITY;
    let mut red = quad(0., [0.25, 0., 0., 1.], GltfAlphaMode::Opaque);
    for vertex in &mut red.mesh.vertices {
        vertex.normal = Vec3::Z.extend(0.);
    }
    let selected = Fixture::new(vec![
        red,
        quad(0., [0., 0., 0.75, 1.], GltfAlphaMode::Opaque),
        // This omitted frontmost chunk would hide both selected parts if a
        // single-part submission accidentally collected its parent model.
        quad(1., [0., 1., 0., 1.], GltfAlphaMode::Opaque),
    ]);
    let left =
        Mat4::from_scale_rotation_translation(Vec3::splat(0.3), Quat::IDENTITY, vec3(-0.5, 0., 0.));
    let right =
        Mat4::from_scale_rotation_translation(Vec3::splat(0.3), Quat::IDENTITY, vec3(0.5, 0., 0.));
    telemetry::set_renderer_diagnostics_enabled(true);
    let mut cpu = None;
    for geometry in [Geometry::Cpu, Geometry::Retained] {
        // Repeat retained draws with the very same borrowed parts. They must
        // remain independently movable without allocating new GPU buffers.
        for _ in 0..2 {
            clear_background(BLACK);
            telemetry::reset_renderer_diagnostics();
            let mut frame = renderer.begin(camera.position, -Vec3::Z).unwrap();
            frame.set_output(GltfColorOutput::Linear);
            submit_part(&mut frame, geometry, &selected, 0, left).unwrap();
            submit_part(&mut frame, geometry, &selected, 1, right).unwrap();
            assert_eq!(
                frame.draw(),
                GltfMaterialStats {
                    opaque_draws: 2,
                    ..Default::default()
                }
            );
            let image = read(target);
            pixel(
                &image,
                16,
                [64, 0, 0, 255],
                "selected red moves left with its factor",
            );
            pixel(&image, 32, [0, 0, 0, 255], "omitted part stays hidden");
            pixel(
                &image,
                48,
                [0, 0, 191, 255],
                "selected blue moves right with its factor",
            );
            if let Some(cpu) = &cpu {
                same_pixels(&image, cpu, "independently transformed selected parts");
            } else {
                cpu = Some(image);
            }
            let counters = telemetry::renderer_diagnostics().counters;
            assert_eq!(counters.retained_buffer_uploads, 0);
            assert_eq!(counters.retained_buffer_upload_bytes, 0);
            if matches!(geometry, Geometry::Retained) {
                assert_eq!(counters.streaming_buffer_uploads, 0);
                assert_eq!(counters.streaming_buffer_upload_bytes, 0);
            }
        }
    }
    telemetry::set_renderer_diagnostics_enabled(false);

    // The frame's reflected parent must compose before the part transform:
    // .25 + (-.5 * -.5) = .5. Lighting and precise surface factors still apply.
    let parent = Mat4::from_scale_rotation_translation(
        vec3(-0.5, 0.5, 1.),
        Quat::IDENTITY,
        vec3(0.25, 0., 0.),
    );
    unsafe { get_internal_gl().quad_gl.push_model_matrix(parent) };
    for geometry in [Geometry::Cpu, Geometry::Retained] {
        clear_background(BLACK);
        let mut frame = renderer.begin(camera.position, -Vec3::Z).unwrap();
        frame.set_output(GltfColorOutput::Linear);
        frame
            .set_lighting(Some(GltfLighting {
                hemisphere_up: Vec3::Y,
                sky_color: Vec3::ZERO,
                ground_color: Vec3::ZERO,
                directional_direction: Vec3::Z,
                directional_color: Vec3::splat(0.5),
            }))
            .unwrap();
        submit_part(&mut frame, geometry, &selected, 0, left).unwrap();
        assert_eq!(frame.draw().opaque_draws, 1);
        let image = read(target);
        pixel(
            &image,
            16,
            [0, 0, 0, 255],
            "selected parent composition vacates left",
        );
        pixel(
            &image,
            48,
            [32, 0, 0, 255],
            "selected reflected parent and lighting",
        );
    }
    unsafe { get_internal_gl().quad_gl.pop_model_matrix() };

    let glass = Fixture::new(vec![
        quad(0.5, [1., 0., 0., 0.5], GltfAlphaMode::Blend),
        quad(-0.5, [0., 0., 1., 0.5], GltfAlphaMode::Blend),
    ]);
    // Part transforms put three blue layers behind three red layers despite
    // the submission order. Global source-over gives red 7/8, blue 7/64.
    clear_background(BLACK);
    let mut frame = renderer.begin(camera.position, -Vec3::Z).unwrap();
    frame.set_output(GltfColorOutput::Linear);
    frame
        .submit_retained_part(
            &glass.retained.parts[0],
            Mat4::from_translation(Vec3::Z * 0.5),
        )
        .unwrap();
    frame
        .submit(&glass.cpu, Mat4::from_translation(Vec3::Z * 0.25))
        .unwrap();
    frame
        .submit_part(&glass.cpu.parts[1], Mat4::from_translation(-Vec3::Z * 0.25))
        .unwrap();
    frame.submit_retained(&glass.retained, identity).unwrap();
    assert_eq!(frame.draw().blend_draws, 6);
    pixel(
        &read(target),
        32,
        [223, 0, 28, 255],
        "selected and whole models sort globally",
    );

    let ties = Fixture::new(vec![
        quad(0., [1., 0., 0., 0.5], GltfAlphaMode::Blend),
        quad(0., [0., 1., 0., 0.5], GltfAlphaMode::Blend),
    ]);
    let red_tie = Fixture::new(vec![quad(0., [1., 0., 0., 0.5], GltfAlphaMode::Blend)]);
    let blue_tie = Fixture::new(vec![quad(0., [0., 0., 1., 0.5], GltfAlphaMode::Blend)]);
    for reverse in [false, true] {
        clear_background(BLACK);
        let mut frame = renderer.begin(camera.position, -Vec3::Z).unwrap();
        frame.set_output(GltfColorOutput::Linear);
        if reverse {
            frame.submit_retained(&red_tie.retained, identity).unwrap();
            frame
                .submit_retained_part(&ties.retained.parts[1], identity)
                .unwrap();
            frame.submit(&blue_tie.cpu, identity).unwrap();
            frame.submit_part(&ties.cpu.parts[0], identity).unwrap();
        } else {
            frame.submit_part(&ties.cpu.parts[0], identity).unwrap();
            frame.submit(&blue_tie.cpu, identity).unwrap();
            frame
                .submit_retained_part(&ties.retained.parts[1], identity)
                .unwrap();
            frame.submit_retained(&red_tie.retained, identity).unwrap();
        }
        assert_eq!(frame.draw().blend_draws, 4);
        pixel(
            &read(target),
            32,
            if reverse {
                [143, 32, 64, 255]
            } else {
                [143, 64, 32, 255]
            },
            "equal-depth selected and whole submissions keep their order",
        );
    }

    for geometry in [Geometry::Cpu, Geometry::Retained] {
        clear_background(BLACK);
        let mut frame = renderer.begin(camera.position, -Vec3::Z).unwrap();
        frame.set_output(GltfColorOutput::Linear);
        submit_part(&mut frame, geometry, &selected, 0, left).unwrap();
        for invalid in [
            Mat4::from_cols_array(&[f32::NAN; 16]),
            Mat4::from_translation(Vec3::splat(f32::INFINITY)),
            Mat4::perspective_rh_gl(1., 1., 0.1, 10.),
        ] {
            assert!(submit_part(&mut frame, geometry, &selected, 2, invalid).is_err());
        }
        submit_part(&mut frame, geometry, &selected, 1, right).unwrap();
        assert_eq!(
            frame.draw(),
            GltfMaterialStats {
                opaque_draws: 2,
                ..Default::default()
            },
            "selected rejection appends nothing, even skipped draws"
        );
        let image = read(target);
        pixel(
            &image,
            16,
            [64, 0, 0, 255],
            "valid part before rejection survives",
        );
        pixel(
            &image,
            48,
            [0, 0, 191, 255],
            "valid part after rejection survives",
        );
    }

    clear_background(BLACK);
    {
        let mut cancelled = renderer.begin(camera.position, -Vec3::Z).unwrap();
        cancelled.submit_part(&selected.cpu.parts[0], left).unwrap();
        cancelled
            .submit_retained_part(&selected.retained.parts[1], right)
            .unwrap();
    }
    let image = read(target);
    pixel(
        &image,
        16,
        [0, 0, 0, 255],
        "dropping a frame cancels a selected CPU part",
    );
    pixel(
        &image,
        48,
        [0, 0, 0, 255],
        "dropping a frame cancels a selected retained part",
    );
    macroquad::logging::info!("GLTF_SELECTED_PART_FIXTURE_PASS");
}
