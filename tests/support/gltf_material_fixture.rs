//! Tiny generated geometry/textures, shared by the native test and browser
//! check. Pixel expectations below are independent material/compositing
//! calculations; CPU/retained equality alone is not a correctness oracle.

use macroquad::prelude::*;

#[path = "gltf_lighting_fixture.rs"]
mod lighting;

#[path = "gltf_selected_part_fixture.rs"]
mod selected_parts;

#[path = "gltf_shading_fixture.rs"]
mod shading;

#[derive(Clone, Copy, Debug)]
enum Geometry {
    Cpu,
    Retained,
}

struct Fixture {
    cpu: GltfModel,
    retained: RetainedGltfModel,
}

impl Fixture {
    fn new(parts: Vec<GltfDrawMesh>) -> Self {
        let cpu = GltfModel {
            bounds_min: parts
                .iter()
                .fold(Vec3::splat(f32::INFINITY), |b, p| b.min(p.bounds_min)),
            bounds_max: parts
                .iter()
                .fold(Vec3::splat(f32::NEG_INFINITY), |b, p| b.max(p.bounds_max)),
            stats: GltfModelStats {
                nodes: parts.len(),
                meshes: parts.len(),
                draw_calls: parts.len(),
                triangles: parts.len() * 2,
                textures: 0,
            },
            root_names: vec!["generated pixel fixture".to_owned()],
            parts,
        };
        let retained = RetainedGltfModel::from_model(cpu.clone(), false).unwrap();
        Self { cpu, retained }
    }
}

fn quad(z: f32, color: [f32; 4], alpha_mode: GltfAlphaMode) -> GltfDrawMesh {
    GltfDrawMesh {
        mesh: Mesh {
            // Deliberately disagree with the precise surface factor. The
            // loader's baked colors must not multiply that factor a second
            // time or quantize it in the opt-in material path.
            vertices: vec![
                Vertex::new(-0.875, -0.875, z, 0., 0., MAGENTA),
                Vertex::new(0.875, -0.875, z, 1., 0., MAGENTA),
                Vertex::new(0.875, 0.875, z, 1., 1., MAGENTA),
                Vertex::new(-0.875, 0.875, z, 0., 1., MAGENTA),
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
            texture: None,
        },
        emissive_texture: None,
        surface: GltfSurface {
            name: Some("generated material".to_owned()),
            base_color: color,
            emissive: [0.; 3],
            metallic: 0.,
            roughness: 1.,
            transmission: 0.,
            alpha_mode,
            alpha_cutoff: 0.5,
            double_sided: false,
            unlit: false,
        },
        node: 0,
        name: "quad".to_owned(),
        path: "generated/quad".to_owned(),
        center: vec3(0., 0., z),
        bounds_min: vec3(-0.875, -0.875, z),
        bounds_max: vec3(0.875, 0.875, z),
    }
}

fn texture(bytes: &[u8]) -> Texture2D {
    let texture = Texture2D::from_rgba8((bytes.len() / 4) as u16, 1, bytes);
    texture.set_filter(FilterMode::Nearest);
    texture
}

fn read(target: &RenderTarget) -> Image {
    unsafe { get_internal_gl().flush() };
    target.texture.get_texture_data()
}

fn pixel(image: &Image, x: usize, expected: [u8; 4], label: &str) {
    let offset = (32 * image.width as usize + x) * 4;
    let actual = &image.bytes[offset..offset + 4];
    for channel in 0..4 {
        assert!(
            actual[channel].abs_diff(expected[channel]) <= 2,
            "{label}: pixel ({x}, 32), channel {channel}: expected {expected:?}, got {actual:?}"
        );
    }
}

fn same_pixels(actual: &Image, expected: &Image, label: &str) {
    assert_eq!(actual.width, expected.width);
    assert_eq!(actual.height, expected.height);
    for (offset, (actual, expected)) in actual.bytes.iter().zip(&expected.bytes).enumerate() {
        assert!(
            actual.abs_diff(*expected) <= 2,
            "{label}: CPU/retained pixel {} channel {}: expected {expected}, got {actual}",
            offset / 4,
            offset % 4
        );
    }
}

fn draw(
    renderer: &mut GltfMaterialRenderer,
    geometry: Geometry,
    output: GltfColorOutput,
    instances: &[(&Fixture, Mat4)],
) -> GltfMaterialStats {
    let mut frame = renderer.begin(vec3(0., 0., 4.), -Vec3::Z).unwrap();
    frame.set_output(output);
    for (fixture, transform) in instances {
        match geometry {
            Geometry::Cpu => frame.submit(&fixture.cpu, *transform).unwrap(),
            Geometry::Retained => frame
                .submit_retained(&fixture.retained, *transform)
                .unwrap(),
        }
    }
    frame.draw()
}

fn check_scene(
    renderer: &mut GltfMaterialRenderer,
    target: &RenderTarget,
    instances: &[(&Fixture, Mat4)],
    output: GltfColorOutput,
    counts: [usize; 3],
    expected: &[(usize, [u8; 4])],
    label: &str,
) {
    let mut cpu = None;
    for geometry in [Geometry::Cpu, Geometry::Retained] {
        clear_background(BLACK);
        let stats = draw(renderer, geometry, output, instances);
        assert_eq!(
            [stats.opaque_draws, stats.mask_draws, stats.blend_draws],
            counts,
            "{label}: draw counts for {geometry:?}"
        );
        let image = read(target);
        for (x, expected) in expected {
            pixel(&image, *x, *expected, label);
        }
        if let Some(cpu) = &cpu {
            same_pixels(&image, cpu, label);
        } else {
            cpu = Some(image);
        }
    }
}

pub async fn run() {
    let target = render_target_ex(
        64,
        64,
        RenderTargetParams {
            depth: true,
            ..Default::default()
        },
    );
    let camera = Camera3D {
        position: vec3(0., 0., 4.),
        target: Vec3::ZERO,
        up: Vec3::Y,
        projection: Projection::Orthographics,
        fovy: 2.,
        aspect: Some(1.),
        z_near: 0.1,
        z_far: 10.,
        render_target: Some(target.clone()),
        ..Default::default()
    };
    set_camera(&camera);
    let mut renderer = GltfMaterialRenderer::new().unwrap();
    let identity = Mat4::IDENTITY;

    // OPAQUE ignores zero alpha and still writes depth. Front submitted first
    // must occlude the later blue geometry; blending or absent depth writes
    // would both produce a different pixel.
    let opaque = Fixture::new(vec![
        quad(0.5, [1., 0., 0., 0.], GltfAlphaMode::Opaque),
        quad(0., [0., 0., 1., 1.], GltfAlphaMode::Opaque),
    ]);
    check_scene(
        &mut renderer,
        &target,
        &[(&opaque, identity)],
        GltfColorOutput::Linear,
        [2, 0, 0],
        &[(32, [255, 0, 0, 255])],
        "opaque ignores alpha and writes depth",
    );

    // Adjacent texels straddle the cutoff. The rejected texel must not write
    // depth, while the accepted texel writes depth and opaque output alpha.
    let mut cutout = quad(0.5, [0., 1., 0., 1.], GltfAlphaMode::Mask);
    cutout.mesh.texture = Some(texture(&[255, 255, 255, 127, 255, 255, 255, 128]));
    let mask_only = Fixture::new(vec![cutout.clone()]);
    let mask = Fixture::new(vec![
        cutout,
        quad(0., [0., 0., 1., 1.], GltfAlphaMode::Opaque),
    ]);
    check_scene(
        &mut renderer,
        &target,
        &[(&mask, identity)],
        GltfColorOutput::Linear,
        [1, 1, 0],
        &[(16, [0, 0, 255, 255]), (48, [0, 255, 0, 255])],
        "mask texture cutoff and depth",
    );
    let cutoff_equal = Fixture::new(vec![quad(0., [0., 1., 0., 0.5], GltfAlphaMode::Mask)]);
    check_scene(
        &mut renderer,
        &target,
        &[(&cutoff_equal, identity)],
        GltfColorOutput::Linear,
        [0, 1, 0],
        &[(32, [0, 255, 0, 255])],
        "mask cutoff equality is visible",
    );

    // One model has separated blue/red layers; two translated instances
    // interleave them. Globally correct back-to-front order is blue, blue,
    // red, red. Sorting each instance individually gives [159, 0, 80].
    let glass = Fixture::new(vec![
        quad(0.5, [1., 0., 0., 0.5], GltfAlphaMode::Blend),
        quad(-0.5, [0., 0., 1., 0.5], GltfAlphaMode::Blend),
    ]);
    let offset = Mat4::from_translation(vec3(0., 0., 0.25));
    for instances in [
        [(&glass, offset), (&glass, identity)],
        [(&glass, identity), (&glass, offset)],
    ] {
        check_scene(
            &mut renderer,
            &target,
            &instances,
            GltfColorOutput::Linear,
            [0, 0, 4],
            &[(32, [191, 0, 48, 255])],
            "global transparency across two instances",
        );
    }
    // The same collector also mixes CPU and retained instances.
    clear_background(BLACK);
    let mut frame = renderer.begin(camera.position, -Vec3::Z).unwrap();
    frame.set_output(GltfColorOutput::Linear);
    frame.submit(&glass.cpu, offset).unwrap();
    frame.submit_retained(&glass.retained, identity).unwrap();
    assert_eq!(frame.draw().blend_draws, 4);
    pixel(
        &read(&target),
        32,
        [191, 0, 48, 255],
        "mixed geometry sorting",
    );

    // Coincident transparent chunk centers deliberately have equal depth.
    // Source-over is order-dependent, so these independent pixel results also
    // verify that the promised submission order survives a stable sort.
    let red_tie = Fixture::new(vec![quad(0., [1., 0., 0., 0.5], GltfAlphaMode::Blend)]);
    let blue_tie = Fixture::new(vec![quad(0., [0., 0., 1., 0.5], GltfAlphaMode::Blend)]);
    for (instances, expected) in [
        (
            [(&red_tie, identity), (&blue_tie, identity)],
            [64, 0, 128, 255],
        ),
        (
            [(&blue_tie, identity), (&red_tie, identity)],
            [128, 0, 64, 255],
        ),
    ] {
        check_scene(
            &mut renderer,
            &target,
            &instances,
            GltfColorOutput::Linear,
            [0, 0, 2],
            &[(32, expected)],
            "equal-depth transparency preserves submission order",
        );
    }

    let front_glass = Fixture::new(vec![quad(0.5, [1., 0., 0., 0.5], GltfAlphaMode::Blend)]);
    let rear_solid = Fixture::new(vec![quad(0., [0., 0., 1., 1.], GltfAlphaMode::Opaque)]);
    for geometry in [Geometry::Cpu, Geometry::Retained] {
        clear_background(BLACK);
        draw(
            &mut renderer,
            geometry,
            GltfColorOutput::Linear,
            &[(&mask_only, identity)],
        );
        // A separate later pass tests depth effects independently of the
        // collector's opaque-before-mask scheduling. Rejected texels must
        // leave room for blue; accepted texels must continue occluding blue.
        draw(
            &mut renderer,
            geometry,
            GltfColorOutput::Linear,
            &[(&rear_solid, identity)],
        );
        let image = read(&target);
        pixel(
            &image,
            16,
            [0, 0, 255, 255],
            "discarded mask leaves depth untouched",
        );
        pixel(&image, 48, [0, 255, 0, 255], "accepted mask writes depth");

        clear_background(BLACK);
        draw(
            &mut renderer,
            geometry,
            GltfColorOutput::Linear,
            &[(&front_glass, identity)],
        );
        pixel(&read(&target), 32, [128, 0, 0, 255], "straight alpha blend");
        // Do not clear depth here. A later opaque pass behind the glass can
        // replace it only if the first BLEND pass left depth untouched.
        draw(
            &mut renderer,
            geometry,
            GltfColorOutput::Linear,
            &[(&rear_solid, identity)],
        );
        pixel(
            &read(&target),
            32,
            [0, 0, 255, 255],
            "blend never writes depth",
        );

        clear_background(Color::new(0., 0., 0., 0.));
        draw(
            &mut renderer,
            geometry,
            GltfColorOutput::Linear,
            &[(&glass, identity)],
        );
        pixel(
            &read(&target),
            32,
            [128, 0, 64, 191],
            "coverage alpha accumulation",
        );
    }
    let occluder = Fixture::new(vec![quad(0., [0., 1., 0., 1.], GltfAlphaMode::Opaque)]);
    check_scene(
        &mut renderer,
        &target,
        &[(&glass, identity), (&occluder, identity)],
        GltfColorOutput::Linear,
        [1, 0, 2],
        &[(32, [128, 128, 0, 255])],
        "transparent fragments still test opaque depth",
    );

    // Standard sRGB transfer, then linear factors, then output encoding:
    // decode([128,64,32]/255) * [.25,.5,.75] encodes to [66,44,27].
    // Alpha remains linear and does not affect OPAQUE.
    let mut base = quad(0., [0.25, 0.5, 0.75, 0.75], GltfAlphaMode::Opaque);
    base.mesh.texture = Some(texture(&[128, 64, 32, 128]));
    let base = Fixture::new(vec![base]);
    check_scene(
        &mut renderer,
        &target,
        &[(&base, identity)],
        GltfColorOutput::Srgb,
        [1, 0, 0],
        &[(32, [66, 44, 27, 255])],
        "sRGB base texture and linear factor",
    );
    clear_background(BLACK);
    let mut default_output = renderer.begin(camera.position, -Vec3::Z).unwrap();
    default_output.submit(&base.cpu, identity).unwrap();
    default_output.draw();
    pixel(
        &read(&target),
        32,
        [66, 44, 27, 255],
        "default output is sRGB",
    );
    check_scene(
        &mut renderer,
        &target,
        &[(&base, identity)],
        GltfColorOutput::Linear,
        [1, 0, 0],
        &[(32, [14, 7, 3, 255])],
        "linear output option",
    );
    let mut emission = quad(0., [0.; 4], GltfAlphaMode::Opaque);
    emission.surface.emissive = [0.25, 0.5, 0.75];
    emission.emissive_texture = Some(texture(&[64, 32, 8, 0]));
    let emission = Fixture::new(vec![emission]);
    check_scene(
        &mut renderer,
        &target,
        &[(&emission, identity)],
        GltfColorOutput::Srgb,
        [1, 0, 0],
        &[(32, [30, 20, 6, 255])],
        "emissive sRGB texture with linear factor and ignored texture alpha",
    );
    let mut combined = quad(0., [0.5, 0.5, 0.5, 1.], GltfAlphaMode::Opaque);
    combined.mesh.texture = Some(texture(&[8, 16, 32, 255]));
    combined.surface.emissive = [0.25; 3];
    combined.emissive_texture = Some(texture(&[32, 16, 8, 255]));
    let combined = Fixture::new(vec![combined]);
    check_scene(
        &mut renderer,
        &target,
        &[(&combined, identity)],
        GltfColorOutput::Srgb,
        [1, 0, 0],
        &[(32, [15, 13, 22, 255])],
        "base plus emission added in linear space",
    );
    let mut dark = quad(0., [0.; 4], GltfAlphaMode::Opaque);
    dark.surface.emissive = [1.; 3];
    dark.emissive_texture = Some(texture(&[4, 4, 4, 255, 8, 8, 8, 255]));
    let dark = Fixture::new(vec![dark]);
    check_scene(
        &mut renderer,
        &target,
        &[(&dark, identity)],
        GltfColorOutput::Srgb,
        [1, 0, 0],
        &[(16, [4, 4, 4, 255]), (48, [8, 8, 8, 255])],
        "emissive dark detail survives conversion",
    );
    let mut factor_only = quad(0., [0.; 4], GltfAlphaMode::Opaque);
    factor_only.surface.emissive = [0.25, 0.5, 0.75];
    let factor_only = Fixture::new(vec![factor_only]);
    check_scene(
        &mut renderer,
        &target,
        &[(&factor_only, identity)],
        GltfColorOutput::Linear,
        [1, 0, 0],
        &[(32, [64, 128, 191, 255])],
        "missing emissive texture is white",
    );
    let mut unlit = quad(0., [0., 1., 0., 1.], GltfAlphaMode::Opaque);
    unlit.surface.unlit = true;
    unlit.surface.emissive = [1., 0., 0.];
    let unlit = Fixture::new(vec![unlit]);
    check_scene(
        &mut renderer,
        &target,
        &[(&unlit, identity)],
        GltfColorOutput::Linear,
        [1, 0, 0],
        &[(32, [0, 255, 0, 255])],
        "unlit material ignores emissive",
    );
    let tiny_factor = Fixture::new(vec![quad(
        0.,
        [0.001, 0.002, 0.003, 1.],
        GltfAlphaMode::Opaque,
    )]);
    check_scene(
        &mut renderer,
        &target,
        &[(&tiny_factor, identity)],
        GltfColorOutput::Srgb,
        [1, 0, 0],
        &[(32, [3, 7, 10, 255])],
        "surface factor keeps precision below one vertex color step",
    );

    let front = Fixture::new(vec![quad(0., [1., 0., 0., 1.], GltfAlphaMode::Opaque)]);
    let mut back = quad(0., [1., 0., 0., 1.], GltfAlphaMode::Opaque);
    for indices in back.mesh.indices.chunks_exact_mut(3) {
        indices.swap(1, 2);
    }
    let back_single = Fixture::new(vec![back.clone()]);
    back.surface.double_sided = true;
    let back_double = Fixture::new(vec![back]);
    for (fixture, transform, expected, label) in [
        (&front, identity, [255, 0, 0, 255], "front face"),
        (
            &back_single,
            identity,
            [0, 0, 0, 255],
            "single sided back is culled",
        ),
        (
            &back_double,
            identity,
            [255, 0, 0, 255],
            "double sided back survives",
        ),
        (
            &front,
            Mat4::from_scale(vec3(-1., 1., 1.)),
            [255, 0, 0, 255],
            "reflected instance preserves front face",
        ),
    ] {
        check_scene(
            &mut renderer,
            &target,
            &[(fixture, transform)],
            GltfColorOutput::Linear,
            [1, 0, 0],
            &[(32, expected)],
            label,
        );
    }

    assert!(renderer.begin(camera.position, Vec3::ZERO).is_err());
    assert!(renderer.begin(Vec3::splat(f32::NAN), -Vec3::Z).is_err());
    clear_background(BLACK);
    {
        let mut cancelled = renderer.begin(camera.position, -Vec3::Z).unwrap();
        cancelled.submit(&front.cpu, identity).unwrap();
        cancelled
            .submit_retained(&front.retained, identity)
            .unwrap();
    }
    pixel(
        &read(&target),
        32,
        [0, 0, 0, 255],
        "dropping a collector cancels pending draws",
    );
    for geometry in [Geometry::Cpu, Geometry::Retained] {
        clear_background(BLACK);
        let mut frame = renderer.begin(camera.position, -Vec3::Z).unwrap();
        for invalid in [
            Mat4::from_cols_array(&[f32::NAN; 16]),
            Mat4::perspective_rh_gl(1., 1., 0.1, 10.),
        ] {
            let result = match geometry {
                Geometry::Cpu => frame.submit(&front.cpu, invalid),
                Geometry::Retained => frame.submit_retained(&front.retained, invalid),
            };
            assert!(result.is_err(), "invalid transform for {geometry:?}");
        }
        match geometry {
            Geometry::Cpu => frame.submit(&front.cpu, identity).unwrap(),
            Geometry::Retained => frame.submit_retained(&front.retained, identity).unwrap(),
        }
        assert_eq!(frame.draw().opaque_draws, 1, "rejected submits are atomic");
        pixel(
            &read(&target),
            32,
            [255, 0, 0, 255],
            "valid submit after rejection",
        );
    }

    // A reflected parent matrix changes winding and bounds for both geometry
    // paths. Each material draw must also restore the caller's model stack.
    let parent = Mat4::from_scale_rotation_translation(
        vec3(-0.5, 0.5, 1.),
        Quat::IDENTITY,
        vec3(0.4, 0., 0.),
    );
    unsafe { get_internal_gl().quad_gl.push_model_matrix(parent) };
    check_scene(
        &mut renderer,
        &target,
        &[(&front, identity)],
        GltfColorOutput::Linear,
        [1, 0, 0],
        &[(16, [0, 0, 0, 255]), (48, [255, 0, 0, 255])],
        "reflected parent model stack",
    );
    clear_background(BLACK);
    front.cpu.draw();
    let image = read(&target);
    pixel(&image, 16, [0, 0, 0, 255], "parent matrix restored on left");
    pixel(
        &image,
        48,
        [255, 0, 255, 255],
        "parent matrix and default material restored",
    );
    unsafe { get_internal_gl().quad_gl.pop_model_matrix() };

    selected_parts::run(&mut renderer, &target, &camera);
    lighting::run(&mut renderer, &target, &camera);
    shading::run(&mut renderer, &target, &camera);

    // Drawing restores the default material: subsequent ordinary drawing must
    // use its own vertex color, regardless of the last glTF material factor.
    set_camera(&Camera2D {
        render_target: Some(target.clone()),
        ..Camera2D::from_display_rect(Rect::new(0., 0., 64., 64.))
    });
    draw_rectangle(0., 0., 64., 64., SKYBLUE);
    pixel(
        &read(&target),
        32,
        [102, 191, 255, 255],
        "default material restored",
    );

    // The renderer flushes before returning and clears custom bindings, so
    // final model owners can be dropped before normal frame collection even
    // while the reusable renderer stays alive.
    set_camera(&camera);
    let last_texture = texture(&[23, 47, 89, 255]);
    let last_weak = last_texture.weak_clone();
    let last_emission = texture(&[1, 2, 3, 255]);
    let last_emission_weak = last_emission.weak_clone();
    let mut last_part = quad(0., [1.; 4], GltfAlphaMode::Opaque);
    last_part.mesh.texture = Some(last_texture);
    last_part.emissive_texture = Some(last_emission);
    let last = Fixture::new(vec![last_part]);
    clear_background(BLACK);
    draw(
        &mut renderer,
        Geometry::Retained,
        GltfColorOutput::Srgb,
        &[(&last, identity)],
    );
    drop(last);
    pixel(
        &read(&target),
        32,
        [23, 47, 89, 255],
        "rendered data survives owners",
    );
    set_default_camera();
    next_frame().await;
    assert_eq!(
        last_weak.raw_miniquad_id(),
        Texture2D::empty().raw_miniquad_id(),
        "reusable renderer releases its last base texture"
    );
    assert_eq!(
        last_emission_weak.raw_miniquad_id(),
        Texture2D::empty().raw_miniquad_id(),
        "reusable renderer releases its last emissive texture"
    );
    drop(renderer);
    println!("glTF material fixture: all independent pixel and lifetime checks passed");
}
