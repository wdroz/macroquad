//! Larger deterministic animation, morph and hierarchy workloads. No GPU state.

use super::*;
use macroquad::{
    gltf::{
        GltfAnimationData, GltfAnimationSample, GltfMorphPrimitive, GltfMorphTarget, GltfScene,
    },
    gltf_draw::{GltfLocalTransform, GltfModelPose},
    math::{Mat4, Quat},
};
use serde_json::json;

fn animation_bytes() -> Vec<u8> {
    let mut binary = Vec::new();
    let mut views = Vec::new();
    let mut accessors = Vec::new();
    let times = (0..32).map(|key| key as f32 / 16.).collect::<Vec<_>>();
    let translations = times
        .iter()
        .flat_map(|&time| [time, time * 0.5, -time * 0.25])
        .collect::<Vec<_>>();
    let rotations = (0..32)
        .flat_map(|key| Quat::from_rotation_z(key as f32 * 0.04).to_array())
        .collect::<Vec<_>>();
    let scales = times
        .iter()
        .flat_map(|&time| [1. + time * 0.1, 1., 1. - time * 0.05])
        .collect::<Vec<_>>();
    for (values, width, kind) in [
        (times, 1, "SCALAR"),
        (translations, 3, "VEC3"),
        (rotations, 4, "VEC4"),
        (scales, 3, "VEC3"),
    ] {
        let offset = binary.len();
        binary.extend(values.iter().flat_map(|value| value.to_le_bytes()));
        let view = views.len();
        views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":values.len()*4}));
        accessors.push(
            json!({"bufferView":view,"componentType":5126,"count":values.len()/width,"type":kind}),
        );
    }
    let channels = (0..96)
        .flat_map(|index| {
            // Channel order deliberately differs from document and scene order.
            let node = index * 37 % 96;
            ["translation", "rotation", "scale"]
                .into_iter()
                .enumerate()
                .map(move |(sampler, path)| json!({"sampler":sampler,"target":{"node":node,"path":path}}))
        })
        .collect::<Vec<_>>();
    animation_fixture::pack(
        json!({
            "asset":{"version":"2.0"}, "scene":0,
            "scenes":[{"nodes":(0..96).rev().collect::<Vec<_>>()}],
            "nodes":vec![json!({});96], "buffers":[{"byteLength":binary.len()}],
            "bufferViews":views,"accessors":accessors,
            "animations":[{"samplers":[
                {"input":0,"output":1,"interpolation":"LINEAR"},
                {"input":0,"output":2,"interpolation":"LINEAR"},
                {"input":0,"output":3,"interpolation":"STEP"}
            ],"channels":channels}]
        }),
        binary,
    )
}

fn animation_case(settings: &Settings) -> usize {
    let bytes = animation_bytes();
    let clip = GltfAnimationData::from_bytes(&bytes)
        .unwrap()
        .clips
        .remove(0);
    let times = (0..16).map(|index| index as f32 / 7.).collect::<Vec<_>>();
    let first = clip.sample(-1.).unwrap();
    assert_eq!(first.transforms.len(), 96);
    for (node, &(id, local)) in first.transforms.iter().enumerate() {
        assert_eq!(id, node);
        assert_eq!(local.translation, Vec3::ZERO);
        assert_eq!(local.scale, Vec3::ONE);
        assert_eq!(local.rotation, Quat::IDENTITY);
    }
    let last = clip.sample(10.).unwrap();
    for (_, local) in &last.transforms {
        assert_eq!(local.translation, vec3(1.9375, 0.96875, -0.484375));
        assert!(local
            .rotation
            .abs_diff_eq(Quat::from_rotation_z(31. * 0.04), 1e-6));
    }
    let mut fp = Fingerprint::new();
    fp.bytes(&bytes);
    for &time in &times {
        fp.f32(time);
    }
    run(
        settings,
        "gltf.animation_many_channels",
        "trs96-channel288-key32-time16-v1",
        fp,
        times.len() as u64,
        || {
            let mut digest = Digest::default();
            for &time in black_box(&times) {
                let sample = black_box(clip.sample(black_box(time)).unwrap());
                digest.hits += sample.transforms.len() as u64;
                for index in [0, 47, 95] {
                    let (node, local) = sample.transforms[index];
                    digest.value(node as u64);
                    digest.vec3(local.translation);
                    digest.vec3(local.scale);
                    for component in local.rotation.to_array() {
                        digest.f32(component);
                    }
                }
            }
            digest
        },
    ) as usize
}

fn morph_cases(settings: &Settings) -> usize {
    let positions = (0..4096)
        .map(|index| {
            vec3(
                (index % 64) as f32 / 32. - 1.,
                (index / 64) as f32 / 32.,
                (index % 7) as f32 * 0.05,
            )
        })
        .collect::<Vec<_>>();
    let targets = (0..8)
        .map(|target| GltfMorphTarget {
            position_deltas: (0..positions.len())
                .map(|index| {
                    vec3(
                        (index % 11) as f32 * 0.001,
                        target as f32 * 0.003,
                        -((index % 5) as f32) * 0.002,
                    )
                })
                .collect(),
            normal_deltas: vec![vec3(target as f32 * 0.02, -0.01, 0.03); positions.len()],
        })
        .collect::<Vec<_>>();
    let mut selected = 0;
    for (benchmark, workload, with_normals, weights) in [
        (
            "gltf.morph_positions",
            "morph4096-target8-dense-positions-v1",
            false,
            [0.35, -0.15, 1.2, 0.4, -0.2, 0.7, 0.25, -0.1],
        ),
        (
            "gltf.morph_normals",
            "morph4096-target8-dense-normals-v1",
            true,
            [0.35, -0.15, 1.2, 0.4, -0.2, 0.7, 0.25, -0.1],
        ),
        (
            "gltf.morph_sparse_normals",
            "morph4096-target8-sparse2-normals-v1",
            true,
            [0., 0., 0.5, 0., 0., 0., 0.75, 0.],
        ),
    ] {
        let mut input = GltfMorphPrimitive {
            positions: positions.clone(),
            normals: if with_normals {
                vec![Vec3::Y; positions.len()]
            } else {
                Vec::new()
            },
            targets: targets.clone(),
        };
        if !with_normals {
            for target in &mut input.targets {
                target.normal_deltas.clear();
            }
        }
        // Vertex-major reference, independent of evaluate's target-major loop.
        let checked = input.evaluate(&weights).unwrap();
        let mut expected_positions = input.positions.clone();
        for (index, position) in expected_positions.iter_mut().enumerate() {
            for (target, &weight) in input.targets.iter().zip(&weights) {
                if weight != 0. {
                    *position += target.position_deltas[index] * weight;
                }
            }
            assert_eq!(
                position.to_array().map(f32::to_bits),
                checked.positions[index].to_array().map(f32::to_bits)
            );
            if with_normals {
                let mut normal = input.normals[index];
                for (target, &weight) in input.targets.iter().zip(&weights) {
                    if weight != 0. {
                        normal += target.normal_deltas[index] * weight;
                    }
                }
                let scale = normal.abs().max_element();
                if scale > 0. {
                    normal = (normal / scale).normalize();
                }
                assert_eq!(
                    normal.to_array().map(f32::to_bits),
                    checked.normals[index].to_array().map(f32::to_bits)
                );
            }
        }
        assert_eq!(checked.bounds, Aabb::from_points(expected_positions));
        let identity = input.evaluate(&[0.; 8]).unwrap();
        assert_eq!(identity.positions, input.positions);
        assert_eq!(identity.normals, input.normals);
        let mut fp = Fingerprint::new();
        for &position in &input.positions {
            fp.vec3(position);
        }
        for &normal in &input.normals {
            fp.vec3(normal);
        }
        for target in &input.targets {
            for &delta in &target.position_deltas {
                fp.vec3(delta);
            }
            for &delta in &target.normal_deltas {
                fp.vec3(delta);
            }
        }
        for &weight in &weights {
            fp.f32(weight);
        }
        selected += run(settings, benchmark, workload, fp, 1, || {
            let output = black_box(input.evaluate(black_box(&weights)).unwrap());
            let mut digest = Digest {
                hits: output.positions.len() as u64,
                ..Default::default()
            };
            for index in [0, 2048, 4095] {
                digest.vec3(output.positions[index]);
                if with_normals {
                    digest.vec3(output.normals[index]);
                }
            }
            let bounds = output.bounds.unwrap();
            digest.vec3(bounds.min);
            digest.vec3(bounds.max);
            digest
        }) as usize;
    }
    selected
}

fn pose_cases(settings: &Settings) -> usize {
    let mut nodes = (0..257)
        .map(|index| json!({"translation":[(index % 5) as f32 * 0.01,0.,0.]}))
        .collect::<Vec<_>>();
    nodes[0]["children"] = json!((0..16).map(|branch| 1 + branch * 16).collect::<Vec<_>>());
    for branch in 0..16 {
        let parent = 1 + branch * 16;
        nodes[parent]["children"] = json!((parent + 1..parent + 16).collect::<Vec<_>>());
    }
    let bytes = serde_json::to_vec(
        &json!({"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],"nodes":nodes}),
    )
    .unwrap();
    let scene = GltfScene::from_bytes(&bytes).unwrap();
    let mut pose = GltfModelPose::from_scene(&scene).hierarchy().unwrap();
    let samples = [0.02, -0.03].map(|offset| GltfAnimationSample {
        transforms: std::iter::once(0)
            .chain((0..16).map(|branch| 1 + branch * 16))
            .map(|node| {
                (
                    node,
                    GltfLocalTransform {
                        translation: vec3(offset + (node % 5) as f32 * 0.01, offset * 0.5, 0.),
                        rotation: Quat::from_rotation_z(offset * node as f32),
                        scale: Vec3::ONE,
                    },
                )
            })
            .collect(),
        weights: Vec::new(),
    });
    let mut fp = Fingerprint::new();
    fp.bytes(&bytes);
    for sample in &samples {
        for &(node, local) in &sample.transforms {
            fp.bytes(&(node as u64).to_le_bytes());
            fp.vec3(local.translation);
            fp.vec3(local.scale);
            for value in local.rotation.to_array() {
                fp.f32(value);
            }
        }
        sample.apply_pose(&mut pose).unwrap();
        // Build expected worlds directly from final locals in scene preorder.
        let mut expected = Vec::new();
        for (index, node) in scene.nodes.iter().enumerate() {
            let mut local = sample
                .transforms
                .iter()
                .find(|(id, _)| *id == index)
                .map(|(_, local)| *local)
                .unwrap_or(GltfLocalTransform {
                    translation: node.translation,
                    rotation: node.rotation,
                    scale: node.scale,
                });
            local.rotation = local.rotation.normalize();
            let world = node
                .parent
                .map(|parent| expected[parent])
                .unwrap_or(Mat4::IDENTITY)
                * local.matrix();
            expected.push(world);
            assert!(pose
                .world(pose.handle(index).unwrap())
                .unwrap()
                .abs_diff_eq(world, 1e-6));
        }
    }
    let mut selected = 0;
    for (benchmark, workload, changed) in [
        (
            "gltf.pose_changed",
            "pose257-branch16-edit17-alternating2-v1",
            true,
        ),
        (
            "gltf.pose_unchanged",
            "pose257-branch16-edit17-identical-v1",
            false,
        ),
    ] {
        selected += run(
            settings,
            benchmark,
            workload,
            fp,
            if changed { 2 } else { 1 },
            || {
                if changed {
                    black_box(&samples[0])
                        .apply_pose(black_box(&mut pose))
                        .unwrap();
                }
                black_box(&samples[1])
                    .apply_pose(black_box(&mut pose))
                    .unwrap();
                let mut digest = Digest {
                    hits: 257,
                    ..Default::default()
                };
                for node in [0, 128, 256] {
                    for component in pose
                        .world(pose.handle(node).unwrap())
                        .unwrap()
                        .to_cols_array()
                    {
                        digest.f32(component);
                    }
                }
                digest
            },
        ) as usize;
    }
    // Exercise the combined public API with transform-only samples. Pose
    // cloning and all parity checks here are setup, outside measured work.
    let mut combined_pose = pose.clone();
    let mut expected_pose = pose.clone();
    for sample in &samples {
        assert!(!sample.apply(&mut combined_pose, None).unwrap());
        sample.apply_pose(&mut expected_pose).unwrap();
        for node in 0..257 {
            let handle = combined_pose.handle(node).unwrap();
            assert_eq!(
                combined_pose
                    .local(handle)
                    .unwrap()
                    .to_cols_array()
                    .map(f32::to_bits),
                expected_pose
                    .local(handle)
                    .unwrap()
                    .to_cols_array()
                    .map(f32::to_bits)
            );
            assert_eq!(
                combined_pose
                    .world(handle)
                    .unwrap()
                    .to_cols_array()
                    .map(f32::to_bits),
                expected_pose
                    .world(handle)
                    .unwrap()
                    .to_cols_array()
                    .map(f32::to_bits)
            );
        }
    }
    for (benchmark, workload, changed) in [
        (
            "gltf.pose_combined_changed",
            "pose257-branch16-edit17-combined-alternating2-v1",
            true,
        ),
        (
            "gltf.pose_combined_unchanged",
            "pose257-branch16-edit17-combined-identical-v1",
            false,
        ),
    ] {
        selected += run(
            settings,
            benchmark,
            workload,
            fp,
            if changed { 2 } else { 1 },
            || {
                if changed {
                    black_box(
                        black_box(&samples[0])
                            .apply(black_box(&mut combined_pose), None)
                            .unwrap(),
                    );
                }
                black_box(
                    black_box(&samples[1])
                        .apply(black_box(&mut combined_pose), None)
                        .unwrap(),
                );
                let mut digest = Digest {
                    hits: 257,
                    ..Default::default()
                };
                for node in [0, 128, 256] {
                    for component in combined_pose
                        .world(combined_pose.handle(node).unwrap())
                        .unwrap()
                        .to_cols_array()
                    {
                        digest.f32(component);
                    }
                }
                digest
            },
        ) as usize;
    }
    selected
}

pub(super) fn cases(settings: &Settings) -> usize {
    animation_case(settings) + morph_cases(settings) + pose_cases(settings)
}
