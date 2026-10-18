//! The lighting extension of the shared native/WebGL material fixture.
//! Expectations use analytic normals and transfer functions, independently of
//! the renderer's transform helper and shader implementation.

use super::*;

fn directional(direction: Vec3) -> GltfLighting {
    GltfLighting {
        hemisphere_up: Vec3::Y,
        sky_color: Vec3::ZERO,
        ground_color: Vec3::ZERO,
        directional_direction: direction,
        directional_color: Vec3::ONE,
    }
}

fn check_lit(
    renderer: &mut GltfMaterialRenderer,
    target: &RenderTarget,
    (fixture, transform): (&Fixture, Mat4),
    lighting: GltfLighting,
    output: GltfColorOutput,
    expected: &[(usize, [u8; 4])],
    label: &str,
) {
    let mut cpu = None;
    for geometry in [Geometry::Cpu, Geometry::Retained] {
        clear_background(BLACK);
        let mut frame = renderer.begin(vec3(0., 0., 4.), -Vec3::Z).unwrap();
        frame.set_output(output);
        frame.set_lighting(Some(lighting)).unwrap();
        match geometry {
            Geometry::Cpu => frame.submit(&fixture.cpu, transform).unwrap(),
            Geometry::Retained => frame.submit_retained(&fixture.retained, transform).unwrap(),
        }
        let stats = frame.draw();
        assert_eq!(stats.opaque_draws, 1, "{label}: {geometry:?}");
        assert_eq!(stats.skipped_draws, 0, "{label}: {geometry:?}");
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

fn lit_quad(normal: Vec3) -> GltfDrawMesh {
    let mut part = quad(0., [1.; 4], GltfAlphaMode::Opaque);
    for vertex in &mut part.mesh.vertices {
        vertex.normal = normal.extend(0.);
    }
    part
}

fn byte(value: f32) -> u8 {
    (value.clamp(0., 1.) * 255.).round() as u8
}

fn gray(value: f32) -> [u8; 4] {
    [byte(value), byte(value), byte(value), 255]
}

// A real in-memory GLB makes the importer participate in this regression.
// An authored smooth normal may differ from the flat quad's geometric normal.
fn imported_quad(reflected: bool) -> Fixture {
    let mut bin = Vec::new();
    for point in [
        [-0.4_f32, -0.4, 0.],
        [0.4, -0.4, 0.],
        [0.4, 0.4, 0.],
        [-0.4, 0.4, 0.],
    ] {
        for component in point {
            bin.extend(component.to_le_bytes());
        }
    }
    for _ in 0..12 {
        bin.extend((1.0_f32 / 3.0_f32.sqrt()).to_le_bytes());
    }
    for index in [0_u16, 1, 2, 0, 2, 3] {
        bin.extend(index.to_le_bytes());
    }
    let document = serde_json::json!({
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0, "scale": [if reflected { -2. } else { 2. }, 1., 0.5]}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "NORMAL": 1}, "indices": 2, "material": 0}]}],
        "materials": [{"pbrMetallicRoughness": {"baseColorFactor": [1., 1., 1., 1.]}}],
        "buffers": [{"byteLength": bin.len()}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 48},
            {"buffer": 0, "byteOffset": 48, "byteLength": 48},
            {"buffer": 0, "byteOffset": 96, "byteLength": 12}
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3", "min": [-0.4, -0.4, 0.], "max": [0.4, 0.4, 0.]},
            {"bufferView": 1, "componentType": 5126, "count": 4, "type": "VEC3"},
            {"bufferView": 2, "componentType": 5123, "count": 6, "type": "SCALAR"}
        ]
    });
    let mut json = serde_json::to_vec(&document).unwrap();
    json.resize((json.len() + 3) & !3, b' ');
    let mut bytes = Vec::new();
    bytes.extend(b"glTF");
    bytes.extend(2_u32.to_le_bytes());
    bytes.extend(((28 + json.len() + bin.len()) as u32).to_le_bytes());
    bytes.extend((json.len() as u32).to_le_bytes());
    bytes.extend(b"JSON");
    bytes.extend(json);
    bytes.extend((bin.len() as u32).to_le_bytes());
    bytes.extend(b"BIN\0");
    bytes.extend(bin);
    let cpu = GltfModel::from_bytes(&bytes).expect("generated imported-normal GLB");
    let retained = RetainedGltfModel::from_model(cpu.clone(), false).unwrap();
    Fixture { cpu, retained }
}

pub(super) fn run(renderer: &mut GltfMaterialRenderer, target: &RenderTarget, camera: &Camera3D) {
    let identity = Mat4::IDENTITY;
    // Node * instance * parent scales are diagonal here, so the expected
    // inverse-transpose is computed by hand: (1,1,1)/(1,.75,.5), or (3,4,6).
    // Reflections at each independent stage must affect X without culling the
    // logical front face or incorrectly reversing the remaining components.
    for imported_reflection in [false, true] {
        let imported = imported_quad(imported_reflection);
        for (instance_sign, parent_sign) in [(1., 1.), (-1., 1.), (1., -1.), (-1., -1.)] {
            let instance = Mat4::from_scale(vec3(0.5 * instance_sign, 1.5, 2.));
            let parent = Mat4::from_scale(vec3(parent_sign, 0.5, 0.5));
            let import_sign = if imported_reflection { -1. } else { 1. };
            let sign = import_sign * instance_sign * parent_sign;
            let expected_normal = vec3(3. * sign, 4., 6.) / 61.0_f32.sqrt();
            unsafe { get_internal_gl().quad_gl.push_model_matrix(parent) };
            for (axis, value) in [
                (Vec3::X * sign, expected_normal.x.abs()),
                (Vec3::Y, expected_normal.y),
                (Vec3::Z, expected_normal.z),
            ] {
                check_lit(
                    renderer,
                    target,
                    (&imported, instance),
                    directional(axis * 7.),
                    GltfColorOutput::Linear,
                    &[(32, gray(value))],
                    "imported, instance and parent nonuniform/reflected normals",
                );
            }
            check_lit(
                renderer,
                target,
                (&imported, instance),
                directional(-Vec3::X * sign),
                GltfColorOutput::Linear,
                &[(32, gray(0.))],
                "reflected normal rejects light on its opposite side",
            );
            unsafe { get_internal_gl().quad_gl.pop_model_matrix() };
        }
    }

    let hemisphere = GltfLighting {
        hemisphere_up: Vec3::Z * 5.,
        sky_color: vec3(0.8, 0.4, 0.2),
        ground_color: vec3(0.2, 0.1, 0.4),
        directional_direction: Vec3::X,
        directional_color: Vec3::ZERO,
    };
    for (normal, expected) in [
        (Vec3::Z, [204, 102, 51, 255]),
        (Vec3::X, [128, 64, 77, 255]),
        (Vec3::ZERO, [128, 64, 77, 255]),
        (-Vec3::Z, [51, 26, 102, 255]),
    ] {
        let fixture = Fixture::new(vec![lit_quad(normal)]);
        check_lit(
            renderer,
            target,
            (&fixture, identity),
            hemisphere,
            GltfColorOutput::Linear,
            &[(32, expected)],
            "hemisphere sky, horizon and ground with arbitrary up axis",
        );
    }

    let mut back = lit_quad(-Vec3::Z);
    for triangle in back.mesh.indices.chunks_exact_mut(3) {
        triangle.swap(1, 2);
    }
    back.surface.double_sided = true;
    let back = Fixture::new(vec![back]);
    for transform in [identity, Mat4::from_scale(vec3(-1., 1., 1.))] {
        check_lit(
            renderer,
            target,
            (&back, transform),
            directional(Vec3::Z),
            GltfColorOutput::Linear,
            &[(32, [255; 4])],
            "double-sided backface reverses normals even under reflection",
        );
    }

    // Textures decode to linear, factors apply exactly once, lighting affects
    // base only, emission is added, then output encodes. Constants below were
    // evaluated independently from the standard sRGB piecewise transfer.
    let mut textured = lit_quad(Vec3::Z);
    textured.surface.base_color = [0.25, 0.5, 0.75, 1.];
    textured.mesh.texture = Some(texture(&[128, 64, 32, 255]));
    textured.surface.emissive = [0.5, 0.25, 0.125];
    textured.emissive_texture = Some(texture(&[32, 16, 8, 0]));
    let textured = Fixture::new(vec![textured]);
    let mut half_light = directional(Vec3::Z);
    half_light.directional_color = Vec3::splat(0.5);
    for (output, expected) in [
        (GltfColorOutput::Linear, [9, 4, 1, 255]),
        (GltfColorOutput::Srgb, [52, 32, 17, 255]),
    ] {
        check_lit(
            renderer,
            target,
            (&textured, identity),
            half_light,
            output,
            &[(32, expected)],
            "lit base and emissive textures use linear factors and explicit output",
        );
    }

    let mut emissive = lit_quad(Vec3::Z);
    emissive.surface.emissive = [1.; 3];
    emissive.emissive_texture = Some(texture(&[4, 4, 4, 0, 8, 8, 8, 0]));
    let emissive = Fixture::new(vec![emissive]);
    check_lit(
        renderer,
        target,
        (&emissive, identity),
        directional(-Vec3::Z),
        GltfColorOutput::Srgb,
        &[(16, [4, 4, 4, 255]), (48, [8, 8, 8, 255])],
        "emissive dark detail survives a completely unlit base",
    );

    let mut unlit = lit_quad(Vec3::ZERO);
    unlit.surface.base_color = [0.25, 0.5, 0.75, 1.];
    unlit.surface.emissive = [1.; 3];
    unlit.surface.unlit = true;
    let unlit = Fixture::new(vec![unlit]);
    check_lit(
        renderer,
        target,
        (&unlit, Mat4::from_scale(vec3(1., 1., 0.))),
        directional(-Vec3::Z),
        GltfColorOutput::Linear,
        &[(32, [64, 128, 191, 255])],
        "KHR unlit ignores lighting, emission and singular normal transform",
    );

    // Changing the option back to None must restore the existing preview even
    // in the same collector. Invalid parameters must not replace the last valid
    // configuration. Both checks are observable independently of API getters.
    let front = Fixture::new(vec![lit_quad(Vec3::Z)]);
    for enabled in [false, true] {
        clear_background(BLACK);
        let mut frame = renderer.begin(camera.position, -Vec3::Z).unwrap();
        frame.set_output(GltfColorOutput::Linear);
        frame.set_lighting(Some(half_light)).unwrap();
        let mut invalid = half_light;
        invalid.sky_color.x = f32::NAN;
        assert!(frame.set_lighting(Some(invalid)).is_err());
        if !enabled {
            frame.set_lighting(None).unwrap();
        }
        frame.submit(&front.cpu, identity).unwrap();
        frame.draw();
        pixel(
            &read(target),
            32,
            if enabled {
                [128, 128, 128, 255]
            } else {
                [255; 4]
            },
            "light setter rejects atomically and None restores preview",
        );
    }
    check_scene(
        renderer,
        target,
        &[(&front, identity)],
        GltfColorOutput::Linear,
        [1, 0, 0],
        &[(32, [255; 4])],
        "new frame restores the default preview after a lit frame",
    );
    for geometry in [Geometry::Cpu, Geometry::Retained] {
        clear_background(BLACK);
        let mut frame = renderer.begin(camera.position, -Vec3::Z).unwrap();
        frame.set_lighting(Some(half_light)).unwrap();
        let singular = Mat4::from_scale(vec3(1., 1., 0.));
        match geometry {
            Geometry::Cpu => frame.submit(&front.cpu, singular).unwrap(),
            Geometry::Retained => frame.submit_retained(&front.retained, singular).unwrap(),
        }
        let counts = frame.draw();
        assert_eq!(
            counts.skipped_draws, 1,
            "lit singular normal transform is skipped"
        );
        assert_eq!(
            counts.opaque_draws, 0,
            "lit singular normal transform is skipped"
        );
        pixel(
            &read(target),
            32,
            [0, 0, 0, 255],
            "singular lit transform emits no draw",
        );
    }

    // Public low-level/custom paths remain usable before and after the opt-in
    // renderer. Their default vertex-color material is already exercised by
    // the parent fixture; a real user shader here proves overrides survive.
    let shader = match unsafe { get_internal_gl().quad_context.info().backend } {
        macroquad::miniquad::Backend::OpenGl => ShaderSource::Glsl {
            vertex: "#version 100\nattribute vec3 position; uniform mat4 Model; uniform mat4 Projection; void main() { gl_Position = Projection * Model * vec4(position, 1.0); }",
            fragment: "#version 100\nprecision lowp float; void main() { gl_FragColor = vec4(0.125, 0.25, 0.5, 1.0); }",
        },
        macroquad::miniquad::Backend::Metal => ShaderSource::Msl { program: r#"
            #include <metal_stdlib>
            using namespace metal;
            struct Vertex { float3 position [[attribute(0)]]; };
            struct RasterizerData { float4 position [[position]]; };
            struct Uniforms { float4x4 Projection; float4x4 Model; float4 _Time; };
            vertex RasterizerData vertexShader(Vertex v [[stage_in]], constant Uniforms& u [[buffer(0)]]) {
                RasterizerData out;
                out.position = u.Projection * u.Model * float4(v.position, 1.0);
                return out;
            }
            fragment float4 fragmentShader(RasterizerData in [[stage_in]]) {
                return float4(0.125, 0.25, 0.5, 1.0);
            }
        "# },
    };
    let custom = load_material(shader, MaterialParams::default()).unwrap();
    for geometry in [Geometry::Cpu, Geometry::Retained] {
        clear_background(BLACK);
        gl_use_material(&custom);
        match geometry {
            Geometry::Cpu => front.cpu.draw(),
            Geometry::Retained => front.retained.draw(identity),
        }
        pixel(
            &read(target),
            32,
            [32, 64, 128, 255],
            "existing custom material overrides remain available",
        );
        gl_use_default_material();
    }
    macroquad::logging::info!("GLTF_LIGHTING_FIXTURE_PASS");
}
