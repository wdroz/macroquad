//! Run `cargo run --features gltf --example gltf_pollable -- model.glb`.
//! Without a path, loads the embedded checkerboard. Space runs, N steps a unit.

#[cfg(feature = "gltf")]
use macroquad::prelude::*;

#[cfg(feature = "gltf")]
#[macroquad::main("Pollable glTF loading")]
async fn main() {
    let bytes = if let Some(path) = std::env::args().nth(1) {
        load_file(&path).await.expect("read GLB")
    } else {
        include_bytes!("assets/textured_quad.glb").to_vec()
    };
    let settings = GltfLoadOptions {
        max_texture_size: Some(512),
        ..Default::default()
    }
    .with_image_limits(GltfImageLimits {
        max_source_pixels: Some(2048 * 2048),
        max_decoded_bytes: Some(16 * 1024 * 1024),
    });
    let mut source = Some(GltfModelLoader::new(&bytes, settings).unwrap());
    let mut upload: Option<RetainedGltfModelLoader> = None;
    let mut model: Option<RetainedGltfModel> = None;
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
            status = String::from("Loading cancelled; R restarts");
        }
        if is_key_pressed(KeyCode::R) {
            // The previous next_frame has finished all draws. Dropping a
            // partial loader has the same ownership cleanup as cancellation.
            set_default_camera();
            gl_use_default_material();
            model = None;
            drop(source.take());
            upload = None;
            retire_unused_render_resources();
            source = Some(GltfModelLoader::new(&bytes, settings).unwrap());
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
            set_default_camera();
        }
        let angle = get_time() as f32 * 3.;
        draw_circle(
            32. + angle.cos() * 10.,
            32. + angle.sin() * 10.,
            4.,
            SKYBLUE,
        );
        draw_text(&status, 58., 38., 22., WHITE);
        draw_text(
            "Space: run/pause | N: one unit | C: cancel | R: restart | Escape: exit",
            18.,
            72.,
            18.,
            LIGHTGRAY,
        );
        draw_text(
            if paused {
                "Construction paused; the loading screen still advances"
            } else {
                "One construction unit per frame; individual units may stall"
            },
            18.,
            98.,
            18.,
            LIGHTGRAY,
        );
        next_frame().await;
    }
}

#[cfg(not(feature = "gltf"))]
fn main() {
    eprintln!("Run with --features gltf");
}
