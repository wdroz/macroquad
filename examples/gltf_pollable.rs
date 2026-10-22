//! Run `cargo run --features gltf --example gltf_pollable -- model.glb`.
//! Without a path, loads the embedded checkerboard. Space runs, N steps a unit.

#[cfg(feature = "gltf")]
use macroquad::prelude::*;

#[cfg(feature = "gltf")]
#[path = "support/gltf_markers_fixture.rs"]
mod fixture;

#[cfg(feature = "gltf")]
#[macroquad::main("Pollable glTF loading")]
async fn main() {
    let bytes = if let Some(path) = std::env::args().nth(1) {
        load_file(&path).await.expect("read GLB")
    } else {
        fixture::bytes()
    };
    let settings = GltfLoadOptions {
        max_texture_size: Some(512),
        ..Default::default()
    }
    .with_image_limits(GltfImageLimits {
        max_source_pixels: Some(2048 * 2048),
        max_decoded_bytes: Some(16 * 1024 * 1024),
    });
    let mut source = Some(GltfModelLoader::new_with_markers(&bytes, settings).unwrap());
    let mut upload: Option<RetainedGltfModelLoader> = None;
    let mut model: Option<RetainedGltfModel> = None;
    let mut markers: Option<GltfMarkers> = None;
    let mut paused = true;
    let mut status = String::from("Ready to parse: press Space or N");

    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        if is_key_pressed(KeyCode::Space) {
            paused = !paused;
        }
        if is_key_pressed(KeyCode::C) {
            if let Some(loader) = &mut source {
                loader.cancel();
            }
            if let Some(loader) = &mut upload {
                loader.cancel();
            }
            source = None;
            upload = None;
            markers = None;
            status = String::from("Loading cancelled; R restarts");
        }
        if is_key_pressed(KeyCode::R) {
            // The previous next_frame has finished all draws. Dropping a
            // partial loader has the same ownership cleanup as cancellation.
            set_default_camera();
            gl_use_default_material();
            model = None;
            markers = None;
            drop(source.take());
            upload = None;
            retire_unused_render_resources();
            source = Some(GltfModelLoader::new_with_markers(&bytes, settings).unwrap());
            status = String::from("Restarted at Parse");
        }

        // Poll at most one construction unit per frame, on the render thread.
        // A single image decode, parse or conversion unit can still be slow.
        if !paused || is_key_pressed(KeyCode::N) {
            if let Some(loader) = &mut source {
                match loader.poll() {
                    Ok(progress) => {
                        status = format!(
                            "{:?}: {} / {}",
                            progress.stage, progress.completed, progress.total
                        );
                        if let Some(cpu) = loader.take_model() {
                            // Taking model, markers, maps and pose is independent.
                            // This example uses Y-up for both drawing and markers.
                            markers = loader.take_markers();
                            // Reuse converted geometry and its uploaded images.
                            upload = Some(RetainedGltfModelLoader::new(cpu, false));
                            source = None;
                        }
                    }
                    Err(error) => {
                        status = error;
                        source = None;
                    }
                }
            } else if let Some(loader) = &mut upload {
                match loader.poll() {
                    Ok(progress) => {
                        status = format!(
                            "{:?}: {} / {}",
                            progress.stage, progress.completed, progress.total
                        );
                        if let Some(ready) = loader.take_model() {
                            status = format!(
                                "Complete: {} chunks, {} textures",
                                ready.parts.len(),
                                ready.stats.textures
                            );
                            model = Some(ready);
                            upload = None;
                        }
                    }
                    Err(error) => {
                        status = error;
                        upload = None;
                    }
                }
            }
        }

        clear_background(Color::new(0.06, 0.08, 0.12, 1.));
        if let Some(model) = &model {
            let center = (model.bounds_min + model.bounds_max) * 0.5;
            let radius = (model.bounds_max - model.bounds_min).length().max(1.);
            let angle = get_time() as f32 * 0.2;
            set_camera(&Camera3D {
                position: center + vec3(angle.sin() * radius, radius * 0.2, angle.cos() * radius),
                target: center,
                up: Vec3::Y,
                z_near: (radius * 0.001).max(0.001),
                z_far: radius * 10.,
                ..Default::default()
            });
            model.draw(Mat4::IDENTITY);
            if let Some(markers) = &markers {
                for spawn in &markers.spawns {
                    draw_sphere(spawn.position, radius * 0.025, None, YELLOW);
                    if let Some(rotation) = markers.marker_rotation(spawn.node) {
                        draw_line_3d(
                            spawn.position,
                            spawn.position + rotation * Vec3::X * radius * 0.15,
                            GREEN,
                        );
                    }
                }
            }
            set_default_camera();
        }
        // Match wrapping and drawing settings at compact and desktop widths.
        let marker_status = markers.as_ref().map_or_else(
            || "Markers: pending (shared parse)".into(),
            |markers| {
                format!(
                    "Markers: {} solids, {} triggers, {} spawns; player node {:?}",
                    markers.solid_colliders().count(),
                    markers.trigger_colliders().count(),
                    markers.spawns.len(),
                    markers
                        .spawns
                        .iter()
                        .find(|spawn| markers.marker_tag(spawn.node) == Some("player"))
                        .map(|spawn| spawn.node)
                )
            },
        );
        let text = format!(
            "{status}\n{marker_status}\nSpace: run/pause | N: one unit\nC: cancel | R: restart | Escape: exit\n{}",
            if paused { "Paused; the loading screen still advances" }
            else { "One unit per frame; individual units may stall" }
        );
        let params = TextParams {
            font_size: 18,
            color: LIGHTGRAY,
            ..Default::default()
        };
        let wrapped = wrap_text(
            &text,
            None,
            params.font_size,
            params.font_scale,
            (screen_width() - 76.).max(1.),
        );
        let dimensions = measure_multiline_text(
            &wrapped,
            params.font,
            params.font_size,
            params.font_scale,
            None,
        );
        // Cover only the measured HUD and its progress-indicator column so
        // bright model pixels cannot reduce text contrast after loading.
        draw_rectangle(
            12.,
            12.,
            (dimensions.width + 58.).min((screen_width() - 24.).max(1.)),
            (dimensions.height + 20.).max(52.),
            Color::new(0.04, 0.055, 0.08, 0.97),
        );
        let angle = get_time() as f32 * 3.;
        draw_circle(
            32. + angle.cos() * 10.,
            32. + angle.sin() * 10.,
            4.,
            SKYBLUE,
        );
        draw_multiline_text_ex(&wrapped, 58., 22. + dimensions.offset_y, None, params);
        next_frame().await;
    }
}

#[cfg(not(feature = "gltf"))]
fn main() {
    eprintln!("Run with --features gltf");
}
