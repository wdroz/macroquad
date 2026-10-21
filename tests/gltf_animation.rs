#![cfg(feature = "gltf")]

#[path = "support/gltf_animation_asset.rs"]
mod asset;

use macroquad::prelude::*;
use serde_json::{json, Value};

fn clip() -> GltfAnimationClip {
    GltfAnimationData::from_bytes(&asset::glb())
        .unwrap()
        .clips
        .remove(0)
}
fn close(a: Vec3, b: Vec3) {
    assert!(a.abs_diff_eq(b, 1e-5), "{a:?} != {b:?}");
}

#[test]
fn child_trs_and_weights_sample_in_scene_order_without_mutating_other_frames() {
    let clip = clip();
    assert_eq!(clip.name(), Some("ChildAndWeight"));
    assert_eq!(clip.duration(), 2.);
    let first = clip.sample(0.).unwrap();
    let middle = clip.sample(1.).unwrap();
    let last = clip.sample(2.).unwrap();
    assert_eq!(middle.transforms[0].0, 1); // document node 0 remaps to scene node 1, after parent 2
    close(middle.transforms[0].1.translation, vec3(-0.1, 0., 0.));
    close(
        middle.transforms[0].1.rotation * Vec3::X,
        vec3(
            std::f32::consts::FRAC_1_SQRT_2,
            std::f32::consts::FRAC_1_SQRT_2,
            0.,
        ),
    );
    assert_eq!(middle.transforms[0].1.scale, Vec3::ONE); // STEP retains left key
    assert_eq!(last.transforms[0].1.scale, vec3(1., 1.5, 1.));
    assert_eq!(middle.weights, [(1, vec![0.5])]);
    assert_eq!(clip.sample(-30.).unwrap(), first);
    assert_eq!(clip.sample(90.).unwrap(), last);
    assert!(clip.sample(f32::NAN).is_err());
    let mut edited = middle;
    edited.weights[0].1[0] = 9.;
    assert_eq!(clip.sample(1.).unwrap().weights[0].1, [0.5]);
    assert_eq!(first.weights[0].1, [0.]);
    assert_eq!(last.weights[0].1, [1.]);
}

#[test]
fn pause_seek_reset_loop_and_independent_clocks_use_caller_time() {
    let mut player = GltfAnimationPlayer::new(clip());
    let second = player.clone();
    player.advance(1.).unwrap();
    player.set_paused(true);
    let frame = player.advance(100.).unwrap();
    assert_eq!(player.time(), 1.);
    assert_eq!(second.time(), 0.);
    assert_eq!(frame.weights[0].1, [0.5]);
    player.seek(0.5).unwrap();
    assert_eq!(player.time(), 0.5);
    for bad in [f32::NAN, f32::INFINITY, -1.] {
        assert!(player.seek(bad).is_err());
        assert!(player.advance(bad).is_err());
        assert_eq!(player.time(), 0.5);
    }
    player.reset();
    assert!(player.is_paused());
    assert_eq!(player.time(), 0.);
    player.set_paused(false);
    player.advance(100.).unwrap();
    assert_eq!(player.time(), 2.);
    player.set_looping(true);
    assert_eq!(player.time(), 0.);
    player.advance(4.5).unwrap();
    assert_eq!(player.time(), 0.5);
    player.advance(f32::MAX).unwrap();
    assert!(player.time().is_finite() && player.time() < 2.);
    player.set_looping(false);
    player.advance(100.).unwrap();
    assert_eq!(player.time(), 2.);
}

#[test]
fn delayed_channels_clamp_and_step_switches_exactly_at_keys() {
    let (mut doc, mut binary) = asset::document();
    replace_f32(&doc, &mut binary, 2, &[2., 4.]);
    doc["accessors"][2]["min"] = json!([2.]);
    doc["accessors"][2]["max"] = json!([4.]);
    let clip = GltfAnimationData::from_bytes(&asset::pack(doc, binary))
        .unwrap()
        .clips
        .remove(0);
    assert_eq!(clip.duration(), 4.);
    assert_eq!(clip.sample(0.).unwrap(), clip.sample(2.).unwrap());
    close(
        clip.sample(3.).unwrap().transforms[0].1.translation,
        vec3(-0.1, 0., 0.),
    );
    assert_eq!(clip.sample(3.999).unwrap().transforms[0].1.scale, Vec3::ONE);
    assert_eq!(
        clip.sample(4.).unwrap().transforms[0].1.scale,
        vec3(1., 1.5, 1.)
    );
}

#[test]
fn posed_child_propagates_and_full_authored_trs_survives_zero_scale_and_axes() {
    let (mut doc, mut binary) = asset::document();
    replace_f32(&doc, &mut binary, 5, &[1., 1., 1., 0., 0., 0.]);
    // Only scale is animated: source translation remains -0.5 and rotation identity.
    doc["animations"][0]["channels"] = json!([{"sampler":2,"target":{"node":0,"path":"scale"}}]);
    let bytes = asset::pack(doc, binary);
    let clip = GltfAnimationData::from_bytes(&bytes)
        .unwrap()
        .clips
        .remove(0);
    let scene = GltfScene::from_bytes(&bytes).unwrap();
    let mut hierarchy = GltfModelPose::from_scene(&scene).hierarchy().unwrap();
    let node = hierarchy.handle(1).unwrap();
    hierarchy
        .set_local(
            node,
            GltfLocalTransform {
                translation: Vec3::splat(12.),
                ..Default::default()
            },
        )
        .unwrap();
    clip.sample(2.).unwrap().apply_pose(&mut hierarchy).unwrap();
    assert!(hierarchy.local_trs(node).is_err()); // zero scale cannot decompose
    clip.sample(0.).unwrap().apply_pose(&mut hierarchy).unwrap();
    close(
        hierarchy.world(node).unwrap().w_axis.truncate(),
        vec3(-0.5, 0.2, 0.),
    );
    let mut z = GltfModelPose::from_scene(&scene)
        .with_up_axis(Vec3::Z)
        .hierarchy()
        .unwrap();
    clip.with_up_axis(Vec3::Z)
        .sample(2.)
        .unwrap()
        .apply_pose(&mut z)
        .unwrap();
    close(
        z.world(z.handle(1).unwrap()).unwrap().w_axis.truncate(),
        vec3(-0.5, 0., 0.2),
    );
    // Root animation conversion applies to translation and rotation.
    let (mut doc, binary) = asset::document();
    doc["animations"][0]["channels"] =
        json!([{"sampler":0,"target":{"node":2,"path":"translation"}}]);
    let root = GltfAnimationData::from_bytes(&asset::pack(doc, binary))
        .unwrap()
        .clips
        .remove(0);
    let sample = root.with_up_axis(Vec3::Z).sample(0.).unwrap();
    close(sample.transforms[0].1.rotation * Vec3::Y, Vec3::Z);
    close(sample.transforms[0].1.translation, vec3(-0.5, 0., 0.));
}

#[test]
fn malformed_channels_accessors_and_unsupported_interpolation_report_errors() {
    for edit in [
        |doc: &mut Value, _: &mut Vec<u8>| {
            let duplicate = doc["animations"][0]["channels"][0].clone();
            doc["animations"][0]["channels"]
                .as_array_mut()
                .unwrap()
                .push(duplicate);
        },
        |doc: &mut Value, _: &mut Vec<u8>| {
            doc["animations"][0]["samplers"][0]["interpolation"] = json!("CUBICSPLINE");
        },
        |doc: &mut Value, _: &mut Vec<u8>| {
            doc["accessors"][3]["count"] = json!(1);
        },
        |doc: &mut Value, _: &mut Vec<u8>| {
            doc["accessors"][3]["byteOffset"] = json!(2);
        },
        |doc: &mut Value, _: &mut Vec<u8>| {
            doc["bufferViews"][3]["byteStride"] = json!(8);
        },
        |doc: &mut Value, _: &mut Vec<u8>| {
            doc["bufferViews"][3]["byteLength"] = json!(8);
        },
        |doc: &mut Value, binary: &mut Vec<u8>| {
            replace_f32(doc, binary, 2, &[1., 1.]);
        },
        |doc: &mut Value, binary: &mut Vec<u8>| {
            replace_f32(doc, binary, 2, &[-1., 2.]);
        },
        |doc: &mut Value, binary: &mut Vec<u8>| {
            replace_f32(doc, binary, 3, &[f32::NAN, 0., 0., 1., 0., 0.]);
        },
        |doc: &mut Value, binary: &mut Vec<u8>| {
            replace_f32(doc, binary, 4, &[0., 0., 0., 0., 0., 0., 0., 1.]);
        },
        |doc: &mut Value, _: &mut Vec<u8>| {
            doc["nodes"][0]
                .as_object_mut()
                .unwrap()
                .remove("translation");
            doc["nodes"][0]["matrix"] = json!([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);
        },
    ] {
        let (mut doc, mut binary) = asset::document();
        edit(&mut doc, &mut binary);
        assert!(GltfAnimationData::from_bytes(&asset::pack(doc, binary)).is_err());
    }
    let (mut doc, binary) = asset::document();
    doc["animations"][0]["samplers"][0]["interpolation"] = json!("CUBICSPLINE");
    let error = GltfAnimationData::from_bytes(&asset::pack(doc, binary))
        .unwrap_err()
        .to_string();
    assert!(error.contains("unsupported interpolation") && error.contains("CUBICSPLINE"));
}

#[test]
fn normalized_integer_sparse_and_interleaved_keys_are_validated() {
    let (mut doc, mut binary) = asset::document();
    let rotation = binary.len();
    binary.extend(
        [0_i16, 0, 0, 32767, 0, 0, 23170, 23170]
            .into_iter()
            .flat_map(i16::to_le_bytes),
    );
    doc["bufferViews"]
        .as_array_mut()
        .unwrap()
        .push(json!({"buffer":0,"byteOffset":rotation,"byteLength":16}));
    doc["accessors"][4] =
        json!({"bufferView":8,"componentType":5122,"normalized":true,"count":2,"type":"VEC4"});
    let indices = binary.len();
    binary.extend([0, 1]);
    binary.extend([0, 0]);
    let values = binary.len();
    binary.extend([0, 255]);
    binary.extend([0, 0]);
    doc["bufferViews"].as_array_mut().unwrap().extend([
        json!({"buffer":0,"byteOffset":indices,"byteLength":2}),
        json!({"buffer":0,"byteOffset":values,"byteLength":2}),
    ]);
    doc["accessors"][6] = json!({"componentType":5121,"normalized":true,"count":2,"type":"SCALAR","sparse":{"count":2,"indices":{"bufferView":9,"componentType":5121},"values":{"bufferView":10}}});
    let translation = binary.len();
    binary.extend(
        [-0.5_f32, 0., 0., 123., 0.3, 0., 0., 456.]
            .into_iter()
            .flat_map(f32::to_le_bytes),
    );
    doc["bufferViews"]
        .as_array_mut()
        .unwrap()
        .push(json!({"buffer":0,"byteOffset":translation,"byteLength":32,"byteStride":16}));
    doc["accessors"][3]["bufferView"] = json!(11);
    let bytes = asset::pack(doc.clone(), binary.clone());
    let sample = GltfAnimationData::from_bytes(&bytes).unwrap().clips[0]
        .sample(1.)
        .unwrap();
    assert_eq!(sample.weights, [(1, vec![0.5])]);
    close(sample.transforms[0].1.translation, vec3(-0.1, 0., 0.));
    assert!((sample.transforms[0].1.rotation.length_squared() - 1.).abs() < 1e-5);
    for bad in [[1, 1], [1, 0], [0, 2]] {
        let mut malformed = binary.clone();
        malformed[indices..indices + 2].copy_from_slice(&bad);
        assert!(GltfAnimationData::from_bytes(&asset::pack(doc.clone(), malformed)).is_err());
    }
}

#[test]
fn outside_scene_channels_are_omitted_and_extreme_finite_interpolation_stays_finite() {
    let (mut doc, mut binary) = asset::document();
    replace_f32(&doc, &mut binary, 3, &[3e38, 0., 0., -3e38, 0., 0.]);
    let sample = GltfAnimationData::from_bytes(&asset::pack(doc.clone(), binary.clone()))
        .unwrap()
        .clips[0]
        .sample(1.)
        .unwrap();
    assert_eq!(sample.transforms[0].1.translation, Vec3::ZERO);
    doc["animations"][0]["channels"] =
        json!([{"sampler":0,"target":{"node":1,"path":"translation"}}]);
    let outside = GltfAnimationData::from_bytes(&asset::pack(doc, binary))
        .unwrap()
        .clips
        .remove(0);
    assert_eq!(outside.duration(), 2.);
    assert_eq!(outside.sample(1.).unwrap(), GltfAnimationSample::default());
}

#[test]
fn failed_multi_node_pose_application_is_atomic() {
    let bytes = asset::glb();
    let scene = GltfScene::from_bytes(&bytes).unwrap();
    let mut pose = GltfModelPose::from_scene(&scene).hierarchy().unwrap();
    let before = pose.world(pose.handle(1).unwrap()).unwrap();
    let mut sample = clip().sample(1.).unwrap();
    sample
        .transforms
        .push((usize::MAX, GltfLocalTransform::default()));
    assert!(sample.apply_pose(&mut pose).is_err());
    assert_eq!(pose.world(pose.handle(1).unwrap()).unwrap(), before);
    assert!(clip().sample(1.).unwrap().apply(&mut pose, None).is_err());
    assert_eq!(pose.world(pose.handle(1).unwrap()).unwrap(), before);
}

#[test]
fn simultaneous_parent_child_edits_validate_final_worlds_without_transient_overflow() {
    let scene = GltfScene::from_bytes(br#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[{"children":[1]},{"scale":[1e20,1e20,1e20]}]}"#).unwrap();
    let mut pose = GltfModelPose::from_scene(&scene).hierarchy().unwrap();
    let parent = pose.handle(0).unwrap();
    // Applying this parent first would multiply its 1e20 by the old child's
    // 1e20, but the final sampled child compensates it and has a finite world.
    assert!(pose
        .set_local(
            parent,
            GltfLocalTransform {
                scale: Vec3::splat(1e20),
                ..Default::default()
            }
        )
        .is_err());
    let sample = GltfAnimationSample {
        transforms: vec![
            (
                0,
                GltfLocalTransform {
                    scale: Vec3::splat(1e20),
                    ..Default::default()
                },
            ),
            (
                1,
                GltfLocalTransform {
                    scale: Vec3::splat(1e-20),
                    ..Default::default()
                },
            ),
        ],
        weights: Vec::new(),
    };
    sample.apply_pose(&mut pose).unwrap();
    assert!(pose
        .world(pose.handle(1).unwrap())
        .unwrap()
        .abs_diff_eq(Mat4::IDENTITY, 1e-5));
}

fn replace_f32(doc: &Value, binary: &mut [u8], accessor: usize, values: &[f32]) {
    let view = doc["accessors"][accessor]["bufferView"].as_u64().unwrap() as usize;
    let offset = doc["bufferViews"][view]["byteOffset"].as_u64().unwrap() as usize;
    for (bytes, value) in binary[offset..].chunks_exact_mut(4).zip(values) {
        bytes.copy_from_slice(&value.to_le_bytes());
    }
}
