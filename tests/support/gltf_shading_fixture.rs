//! Native and WebGL checks for normal and metallic-roughness sampling.
//! Expected colors come from `shade_gltf_surface` and `gltf_tangent_frame`.

use super::*;

fn no_hemi(direction: Vec3) -> GltfLighting {
    GltfLighting {
        hemisphere_up: Vec3::Y,
        sky_color: Vec3::ZERO,
        ground_color: Vec3::ZERO,
        directional_direction: direction,
        directional_color: Vec3::ONE,
    }
}

fn nearest(width: u16, height: u16, bytes: &[u8]) -> Texture2D {
    let texture = Texture2D::from_rgba8(width, height, bytes);
    texture.set_filter(FilterMode::Nearest);
    texture
}

fn map(texture: Texture2D, uvs: Vec<Vec2>, strength: f32) -> GltfDrawMaterialMap {
    GltfDrawMaterialMap {
        texture,
        tex_coord: 0,
        transform: GltfUvTransform::default(),
        strength,
        uvs,
    }
}

fn shaded_part(normal: Vec3, base: [f32; 4]) -> GltfDrawMesh {
    let mut part = quad(0., base, GltfAlphaMode::Opaque);
    for vertex in &mut part.mesh.vertices {
        vertex.normal = normal.extend(0.);
    }
    part
}

fn repeated<T: Copy>(part: &GltfDrawMesh, value: T) -> Vec<T> {
    vec![value; part.mesh.vertices.len()]
}

fn part_maps(
    part: &GltfDrawMesh,
    normal: Option<GltfDrawMaterialMap>,
    metallic_roughness: Option<GltfDrawMaterialMap>,
    tangent: Vec4,
) -> GltfPartMaterialMaps {
    GltfPartMaterialMaps {
        normal,
        metallic_roughness,
        tangents: if tangent == Vec4::ZERO {
            Vec::new()
        } else {
            repeated(part, tangent)
        },
        material_index: Some(0),
        material_name: Some("shaded".into()),
    }
}

fn linear_bytes(color: Vec3) -> [u8; 4] {
    let byte = |channel: f32| (channel.clamp(0., 1.) * 255.).round() as u8;
    [byte(color.x), byte(color.y), byte(color.z), 255]
}

fn near(image: &Image, x: usize, expected: [u8; 4], tolerance: u8, label: &str) {
    let offset = (32 * image.width as usize + x) * 4;
    let actual = &image.bytes[offset..offset + 4];
    for component in 0..4 {
        assert!(
            actual[component].abs_diff(expected[component]) <= tolerance,
            "{label}: pixel ({x}, 32) channel {component}: expected {expected:?} ±{tolerance}, got {actual:?}"
        );
    }
}

struct ShadedScene {
    fixture: Fixture,
    maps: GltfModelMaterialMaps,
    shading: GltfShadingModel,
}

impl ShadedScene {
    fn new(part: GltfDrawMesh, maps: GltfPartMaterialMaps) -> Self {
        let fixture = Fixture::new(vec![part]);
        let maps = GltfModelMaterialMaps { parts: vec![maps] };
        let shading = GltfShadingModel::from_model(&fixture.cpu, &maps).expect("shading upload");
        Self {
            fixture,
            maps,
            shading,
        }
    }

    fn render(
        &self,
        renderer: &mut GltfMaterialRenderer,
        target: &RenderTarget,
        lighting: Option<GltfLighting>,
        transform: Mat4,
        retained: bool,
        label: &str,
    ) -> Image {
        clear_background(BLACK);
        let mut frame = renderer.begin(vec3(0., 0., 4.), -Vec3::Z).unwrap();
        frame.set_output(GltfColorOutput::Linear);
        frame.set_lighting(lighting).unwrap();
        if retained {
            frame
                .submit_retained_part_shaded(
                    &self.fixture.retained.parts[0],
                    &self.shading.parts()[0],
                    transform,
                )
                .unwrap();
        } else {
            frame
                .submit_part_shaded(&self.fixture.cpu.parts[0], &self.maps.parts[0], transform)
                .unwrap();
        }
        let stats = frame.draw();
        assert_eq!(stats.opaque_draws, 1, "{label}");
        assert_eq!(stats.skipped_draws, 0, "{label}");
        read(target)
    }

    fn check(
        &self,
        renderer: &mut GltfMaterialRenderer,
        target: &RenderTarget,
        lighting: Option<GltfLighting>,
        transform: Mat4,
        label: &str,
        probe: impl Fn(&Image, &str),
    ) {
        let mut cpu = None;
        for retained in [false, true] {
            let image = self.render(renderer, target, lighting, transform, retained, label);
            probe(&image, label);
            if let Some(previous) = &cpu {
                same_pixels(&image, previous, label);
            } else {
                cpu = Some(image);
            }
        }
    }
}

fn reference(base: Vec3, metallic: f32, roughness: f32, normal: Vec3, light: Vec3) -> [u8; 4] {
    linear_bytes(shade_gltf_surface(
        base,
        metallic,
        roughness,
        normal,
        Vec3::Z,
        no_hemi(light),
        Vec3::ZERO,
    ))
}

fn normal_from_map(
    geometric: Vec3,
    tangent: Vec4,
    instance: Mat4,
    texel: Vec3,
    scale: f32,
) -> Vec3 {
    let (n, t, b) = gltf_tangent_frame(geometric, tangent.truncate(), tangent.w, instance)
        .expect("tangent frame");
    gltf_tangent_normal(n, t, b, texel / 255., scale)
}

/// 1×1 RGBA PNG, linear normal-map blue-green `(128, 255, 128)`.
const PLUS_Y_NORMAL_PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 104, 248, 223, 240, 31, 0,
    7, 1, 2, 255, 39, 31, 202, 23, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

fn glb(json: serde_json::Value, binary: &[u8]) -> Vec<u8> {
    let mut json = serde_json::to_vec(&json).unwrap();
    json.resize(json.len().next_multiple_of(4), b' ');
    let mut bytes = Vec::new();
    bytes.extend(b"glTF");
    bytes.extend(2_u32.to_le_bytes());
    bytes.extend(((28 + json.len() + binary.len()) as u32).to_le_bytes());
    bytes.extend((json.len() as u32).to_le_bytes());
    bytes.extend(b"JSON");
    bytes.extend(&json);
    bytes.extend((binary.len() as u32).to_le_bytes());
    bytes.extend(b"BIN\0");
    bytes.extend(binary);
    bytes
}

fn normal_mapped_glb(node_transform: Mat4) -> Vec<u8> {
    let mut binary = Vec::new();
    for point in [
        [-0.8_f32, -0.8, 0.],
        [0.8, -0.8, 0.],
        [0.8, 0.8, 0.],
        [-0.8, 0.8, 0.],
    ] {
        for component in point {
            binary.extend(component.to_le_bytes());
        }
    }
    for _ in 0..4 {
        for component in [0.0_f32, 0.0, 1.0] {
            binary.extend(component.to_le_bytes());
        }
    }
    for _ in 0..4 {
        for component in [1.0_f32, 0.0, 0.0, 1.0] {
            binary.extend(component.to_le_bytes());
        }
    }
    for uv in [0.0_f32, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0] {
        binary.extend(uv.to_le_bytes());
    }
    let image_offset = binary.len();
    binary.extend(PLUS_Y_NORMAL_PNG);
    binary.resize(binary.len().next_multiple_of(4), 0);
    let index_offset = binary.len();
    for index in [0_u16, 1, 2, 0, 2, 3] {
        binary.extend(index.to_le_bytes());
    }
    glb(
        serde_json::json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"name": "panel", "mesh": 0, "matrix": node_transform.to_cols_array()}],
            "meshes": [{"primitives": [{"attributes": {
                "POSITION": 0, "NORMAL": 1, "TANGENT": 2, "TEXCOORD_0": 3
            }, "indices": 4, "material": 0}]}],
            "materials": [{
                "name": "paint",
                "pbrMetallicRoughness": {
                    "baseColorFactor": [1.0, 1.0, 1.0, 1.0],
                    "metallicFactor": 0.0,
                    "roughnessFactor": 1.0
                },
                "normalTexture": {"index": 0}
            }],
            "textures": [{"source": 0}],
            "images": [{"name": "normals", "bufferView": 4, "mimeType": "image/png"}],
            "buffers": [{"byteLength": binary.len()}],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": 48},
                {"buffer": 0, "byteOffset": 48, "byteLength": 48},
                {"buffer": 0, "byteOffset": 96, "byteLength": 64},
                {"buffer": 0, "byteOffset": 160, "byteLength": 32},
                {"buffer": 0, "byteOffset": image_offset, "byteLength": PLUS_Y_NORMAL_PNG.len()},
                {"buffer": 0, "byteOffset": index_offset, "byteLength": 12}
            ],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3", "min": [-0.8, -0.8, 0.0], "max": [0.8, 0.8, 0.0]},
                {"bufferView": 1, "componentType": 5126, "count": 4, "type": "VEC3"},
                {"bufferView": 2, "componentType": 5126, "count": 4, "type": "VEC4"},
                {"bufferView": 3, "componentType": 5126, "count": 4, "type": "VEC2"},
                {"bufferView": 5, "componentType": 5123, "count": 6, "type": "SCALAR"}
            ]
        }),
        &binary,
    )
}

pub(super) fn run(renderer: &mut GltfMaterialRenderer, target: &RenderTarget, camera: &Camera3D) {
    set_camera(camera);
    let identity = Mat4::IDENTITY;

    // Preview and unlit submission keep the base color. Metallic 1 would go
    // dark if the shaded response ran.
    let mut preview_part = shaded_part(Vec3::Z, [1., 0., 0., 1.]);
    preview_part.surface.metallic = 1.;
    let mut unlit_part = preview_part.clone();
    unlit_part.surface.unlit = true;
    let wild = map(
        nearest(1, 1, &[255, 128, 128, 255]),
        repeated(&preview_part, Vec2::ZERO),
        1.,
    );
    for (part, label) in [
        (
            preview_part.clone(),
            "preview ignores normal and roughness maps",
        ),
        (unlit_part, "unlit ignores maps while lighting is on"),
    ] {
        let scene = ShadedScene::new(
            part,
            part_maps(
                &preview_part,
                Some(wild.clone()),
                None,
                vec4(1., 0., 0., 1.),
            ),
        );
        let lighting = if label.starts_with("preview") {
            None
        } else {
            Some(no_hemi(Vec3::Z))
        };
        scene.check(
            renderer,
            target,
            lighting,
            identity,
            label,
            |image, label| {
                near(image, 32, [255, 0, 0, 255], 2, label);
            },
        );
    }

    let white = shaded_part(Vec3::Z, [1.; 4]);
    let dielectric = ShadedScene::new(white.clone(), part_maps(&white, None, None, Vec4::ZERO));
    dielectric.check(
        renderer,
        target,
        Some(no_hemi(Vec3::Z)),
        identity,
        "rough dielectric matches the analytic response",
        |image, label| {
            near(
                image,
                32,
                reference(Vec3::ONE, 0., 1., Vec3::Z, Vec3::Z),
                5,
                label,
            );
        },
    );

    let mut metal = shaded_part(Vec3::Z, [1., 0., 0., 1.]);
    metal.surface.metallic = 1.;
    metal.surface.roughness = 1.;
    let metal_scene = ShadedScene::new(metal.clone(), part_maps(&metal, None, None, Vec4::ZERO));
    metal_scene.check(
        renderer,
        target,
        Some(no_hemi(Vec3::Z)),
        identity,
        "rough metal matches the analytic response",
        |image, label| {
            near(
                image,
                32,
                reference(Vec3::X, 1., 1., Vec3::Z, Vec3::Z),
                5,
                label,
            );
        },
    );

    // Both screen sides share UV 0.25, so both sample the left +Z texel.
    // Mesh UVs would put the right side on the +X texel and leave it dark.
    let normal_tex = nearest(2, 1, &[128, 128, 255, 255, 255, 128, 128, 255]);
    let constant_normal = ShadedScene::new(
        white.clone(),
        part_maps(
            &white,
            Some(map(normal_tex, repeated(&white, vec2(0.25, 0.5)), 1.)),
            None,
            vec4(1., 0., 0., 1.),
        ),
    );
    constant_normal.check(
        renderer,
        target,
        Some(no_hemi(Vec3::Z)),
        identity,
        "constant normal-map UVs ignore the mesh UV",
        |image, label| {
            for x in [16, 48] {
                assert!(
                    image.bytes[(32 * image.width as usize + x) * 4] > 200,
                    "{label}: x={x} should sample the +Z texel, got {:?}",
                    &image.bytes[(32 * image.width as usize + x) * 4
                        ..(32 * image.width as usize + x) * 4 + 4]
                );
            }
        },
    );

    // UV 0.75 is the metal texel. Mesh UVs would leave the left side dielectric.
    let mr_tex = nearest(2, 1, &[0, 255, 0, 255, 0, 255, 255, 255]);
    let mut mr_part = shaded_part(Vec3::Z, [1., 0., 0., 1.]);
    mr_part.surface.metallic = 1.;
    mr_part.surface.roughness = 1.;
    let constant_mr = ShadedScene::new(
        mr_part.clone(),
        part_maps(
            &mr_part,
            None,
            Some(map(mr_tex, repeated(&mr_part, vec2(0.75, 0.5)), 1.)),
            vec4(1., 0., 0., 1.),
        ),
    );
    constant_mr.check(
        renderer,
        target,
        Some(no_hemi(Vec3::Z)),
        identity,
        "constant metallic-roughness UVs select the metal texel",
        |image, label| {
            let expected = reference(Vec3::X, 1., 1., Vec3::Z, Vec3::Z);
            for x in [16, 48] {
                near(image, x, expected, 5, label);
            }
        },
    );

    // Shear sends +X to normalize(1, 0.5, 0). Light along +Y is then partial;
    // an untransformed tangent stays perpendicular to that light.
    let mut shear = Mat4::IDENTITY;
    shear.x_axis.y = 0.5;
    let sheared_normal = normal_from_map(
        Vec3::Z,
        vec4(1., 0., 0., 1.),
        shear,
        Vec3::new(255., 128., 128.),
        1.,
    );
    let shear_scene = ShadedScene::new(
        white.clone(),
        part_maps(
            &white,
            Some(map(
                nearest(1, 1, &[255, 128, 128, 255]),
                repeated(&white, Vec2::ZERO),
                1.,
            )),
            None,
            vec4(1., 0., 0., 1.),
        ),
    );
    shear_scene.check(
        renderer,
        target,
        Some(no_hemi(Vec3::Y)),
        shear,
        "sheared instance bends the tangent-space +X normal",
        |image, label| {
            near(
                image,
                32,
                reference(Vec3::ONE, 0., 1., sheared_normal, Vec3::Y),
                8,
                label,
            );
            assert!(
                image.bytes[(32 * image.width as usize + 32) * 4] > 70,
                "{label}: sheared normal should catch +Y light"
            );
        },
    );

    // scale(-2, 1, 1) keeps +Z and turns the bitangent so a +Y map stays +Y.
    let mirror = Mat4::from_scale(vec3(-2., 1., 1.));
    let mirrored_normal = normal_from_map(
        Vec3::Z,
        vec4(1., 0., 0., 1.),
        mirror,
        Vec3::new(128., 255., 128.),
        1.,
    );
    let mirror_scene = ShadedScene::new(
        white.clone(),
        part_maps(
            &white,
            Some(map(
                nearest(1, 1, &[128, 255, 128, 255]),
                repeated(&white, Vec2::ZERO),
                1.,
            )),
            None,
            vec4(1., 0., 0., 1.),
        ),
    );
    mirror_scene.check(
        renderer,
        target,
        Some(no_hemi(Vec3::Y)),
        mirror,
        "mirrored instance keeps a +Y normal map pointing +Y",
        |image, label| {
            near(
                image,
                32,
                reference(Vec3::ONE, 0., 1., mirrored_normal, Vec3::Y),
                8,
                label,
            );
        },
    );
    mirror_scene.check(
        renderer,
        target,
        Some(no_hemi(-Vec3::Y)),
        mirror,
        "mirrored +Y normal map rejects light from -Y",
        |image, label| {
            assert!(
                image.bytes[(32 * image.width as usize + 32) * 4] < 30,
                "{label}: got {}",
                image.bytes[(32 * image.width as usize + 32) * 4]
            );
        },
    );

    // Inverse-transpose of scale(2, 1, 0.5), not the forward scale. The light
    // is perpendicular to the forward-scaled normal and still faces the
    // inverse-transpose normal.
    let geometric = Vec3::new(0., 1., 1.).normalize();
    let scaled = Mat4::from_scale(vec3(2., 1., 0.5));
    let (expected_normal, _, _) =
        gltf_tangent_frame(geometric, Vec3::X, 1., scaled).expect("scaled frame");
    let forward = (scaled.transform_vector3(geometric)).normalize();
    let light = Vec3::new(0., -forward.z, forward.y).normalize();
    assert!(
        expected_normal.dot(light) > 0.4,
        "{}",
        expected_normal.dot(light)
    );
    assert!(forward.dot(light).abs() < 1.0e-4);
    let scaled_part = shaded_part(geometric, [1.; 4]);
    let scaled_scene = ShadedScene::new(
        scaled_part.clone(),
        part_maps(&scaled_part, None, None, vec4(1., 0., 0., 1.)),
    );
    scaled_scene.check(
        renderer,
        target,
        Some(no_hemi(light)),
        scaled,
        "nonuniform scale uses the inverse-transpose normal",
        |image, label| {
            near(
                image,
                32,
                reference(Vec3::ONE, 0., 1., expected_normal, light),
                8,
                label,
            );
            assert!(
                image.bytes[(32 * image.width as usize + 32) * 4] > 80,
                "{label}: forward-scaled normal would be unlit"
            );
        },
    );

    // Back face, double-sided: N and T flip together, so a +Y map follows the
    // stored bitangent instead of taking the mirror/side sign a second time.
    let mut back = shaded_part(-Vec3::Z, [1.; 4]);
    for triangle in back.mesh.indices.as_chunks_mut::<3>().0 {
        triangle.swap(1, 2);
    }
    back.surface.double_sided = true;
    let back_normal = {
        let (n, t, b) = gltf_tangent_frame(-Vec3::Z, Vec3::X, 1., identity).unwrap();
        // The visible side is the back face, so the fragment flips N and T.
        gltf_tangent_normal(-n, -t, b, Vec3::new(128., 255., 128.) / 255., 1.)
    };
    let back_scene = ShadedScene::new(
        back.clone(),
        part_maps(
            &back,
            Some(map(
                nearest(1, 1, &[128, 255, 128, 255]),
                repeated(&back, Vec2::ZERO),
                1.,
            )),
            None,
            vec4(1., 0., 0., 1.),
        ),
    );
    back_scene.check(
        renderer,
        target,
        Some(no_hemi(back_normal)),
        identity,
        "double-sided back face keeps the normal-map bitangent",
        |image, label| {
            near(
                image,
                32,
                reference(Vec3::ONE, 0., 1., back_normal, back_normal),
                8,
                label,
            );
        },
    );

    // Baking a node's shear and applying it to an instance must produce the
    // same tangent frame. A +Y texel follows cross(+Z, normalize(1, .5, 0)),
    // so it catches -X light. Applying the inverse transpose to T would keep
    // that bitangent on +Y and leave the center dark instead.
    let mut imported_shear = Mat4::IDENTITY;
    imported_shear.x_axis.y = 0.5;
    let local_bytes = normal_mapped_glb(identity);
    let node_bytes = normal_mapped_glb(imported_shear);
    let local_assets =
        GltfModel::from_bytes_with_material_maps(&local_bytes, GltfLoadSettings::default())
            .unwrap();
    let node_assets =
        GltfModel::from_bytes_with_material_maps(&node_bytes, GltfLoadSettings::default()).unwrap();
    let local_scene = ShadedScene::new(
        local_assets.model.parts[0].clone(),
        local_assets.material_maps.parts[0].clone(),
    );
    let node_scene = ShadedScene::new(
        node_assets.model.parts[0].clone(),
        node_assets.material_maps.parts[0].clone(),
    );
    let expected = normal_from_map(
        Vec3::Z,
        vec4(1., 0., 0., 1.),
        imported_shear,
        Vec3::new(128., 255., 128.),
        1.,
    );
    for retained in [false, true] {
        let instance_image = local_scene.render(
            renderer,
            target,
            Some(no_hemi(-Vec3::X)),
            imported_shear,
            retained,
            "instance shear normal map",
        );
        let node_image = node_scene.render(
            renderer,
            target,
            Some(no_hemi(-Vec3::X)),
            identity,
            retained,
            "loaded node shear normal map",
        );
        near(
            &node_image,
            32,
            reference(Vec3::ONE, 0., 1., expected, -Vec3::X),
            8,
            "loaded node shear follows the surface edge tangent",
        );
        same_pixels(
            &node_image,
            &instance_image,
            "node and instance shear agree",
        );
    }

    let bytes = normal_mapped_glb(identity);
    let assets =
        GltfModel::from_bytes_with_material_maps(&bytes, GltfLoadSettings::default()).unwrap();
    assert!(assets.material_maps.parts[0].normal.is_some());
    let imported_normal = normal_from_map(
        Vec3::Z,
        assets.material_maps.parts[0].tangents[0],
        identity,
        Vec3::new(128., 255., 128.),
        1.,
    );
    let retained = RetainedGltfModel::from_model(assets.model.clone(), false).unwrap();
    let shading = GltfShadingModel::from_model(&assets.model, &assets.material_maps).unwrap();
    for retained_draw in [false, true] {
        clear_background(BLACK);
        let mut frame = renderer.begin(vec3(0., 0., 4.), -Vec3::Z).unwrap();
        frame.set_output(GltfColorOutput::Linear);
        frame.set_lighting(Some(no_hemi(Vec3::Y))).unwrap();
        if retained_draw {
            frame
                .submit_retained_shaded(&retained, &shading, identity)
                .unwrap();
        } else {
            frame
                .submit_shaded(&assets.model, &assets.material_maps, identity)
                .unwrap();
        }
        assert_eq!(frame.draw().skipped_draws, 0);
        near(
            &read(target),
            32,
            reference(Vec3::ONE, 0., 1., imported_normal, Vec3::Y),
            8,
            "imported normal map",
        );
    }
    clear_background(BLACK);
    let mut frame = renderer.begin(vec3(0., 0., 4.), -Vec3::Z).unwrap();
    frame.set_output(GltfColorOutput::Linear);
    frame.set_lighting(Some(no_hemi(Vec3::Y))).unwrap();
    frame.submit(&assets.model, identity).unwrap();
    frame.draw();
    assert!(
        read(target).bytes[(32 * 64 + 32) * 4] < 30,
        "geometric +Z without the normal map stays dark under +Y light"
    );
}
