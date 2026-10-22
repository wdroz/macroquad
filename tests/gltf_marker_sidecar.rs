#![cfg(feature = "gltf")]

use macroquad::prelude::*;

#[path = "../examples/support/gltf_markers_fixture.rs"]
mod fixture;

fn loader(bytes: &[u8]) -> GltfModelLoader<'_> {
    GltfModelLoader::new_with_markers(bytes, GltfLoadSettings::default()).unwrap()
}

fn complete(loader: &mut GltfModelLoader<'_>) {
    while loader.poll().unwrap().stage != GltfLoadStage::Complete {
        assert!(loader.take_markers().is_none());
    }
}

fn assert_scene(markers: &GltfMarkers, scene: &GltfScene) {
    assert_eq!(markers.colliders, scene.colliders);
    assert_eq!(markers.spawns, scene.spawns);
    assert_eq!(
        markers.solid_colliders().collect::<Vec<_>>(),
        scene.solid_colliders().collect::<Vec<_>>()
    );
    assert_eq!(
        markers.trigger_colliders().collect::<Vec<_>>(),
        scene.trigger_colliders().collect::<Vec<_>>()
    );
    for (index, node) in scene.nodes.iter().enumerate() {
        assert_eq!(markers.marker_tag(index), scene.marker_tag(index));
        assert_eq!(markers.rest_world(index), Some(node.world));
        assert_eq!(markers.marker_rotation(index), scene.marker_rotation(index));
    }
    for index in [scene.nodes.len(), usize::MAX] {
        assert_eq!(markers.marker_tag(index), None);
        assert_eq!(markers.rest_world(index), None);
        assert_eq!(markers.marker_rotation(index), None);
    }
}

#[test]
fn shared_markers_match_scene_shapes_tags_indices_facing_and_axis_conversion() {
    let bytes = fixture::untextured_bytes();
    let scene = GltfScene::from_bytes(&bytes).unwrap();
    let mut load = loader(&bytes);
    assert!(load.take_markers().is_none());
    complete(&mut load);
    let markers = load.take_markers().unwrap();
    assert!(load.take_markers().is_none());
    assert_eq!(markers, GltfMarkers::from_scene(&scene));
    assert_scene(&markers, &scene);
    assert_eq!(scene.nodes.len(), 9);
    assert_eq!(
        markers.colliders.iter().map(|c| c.node).collect::<Vec<_>>(),
        [1, 4, 5, 6]
    );
    assert!(matches!(
        markers.colliders[0].shape,
        GltfColliderShape::Aabb(_)
    ));
    assert!(matches!(
        markers.colliders[1].shape,
        GltfColliderShape::Sphere(_)
    ));
    assert!(matches!(
        markers.colliders[2].shape,
        GltfColliderShape::Capsule(_)
    ));
    assert!(matches!(
        markers.colliders[3].shape,
        GltfColliderShape::Obb(_)
    ));
    assert_eq!(markers.spawns[0].name, markers.spawns[1].name);
    assert_eq!(markers.marker_tag(markers.spawns[0].node), Some("guard"));
    assert_eq!(markers.marker_tag(markers.spawns[1].node), Some("player"));
    assert_eq!(markers.marker_tag(7), Some("label")); // A tag alone is no marker.
    assert_eq!(markers.marker_tag(5), Some(""));
    assert_eq!(markers.marker_tag(8), None); // Non-string tag, singular facing.
    assert_eq!(markers.marker_rotation(8), None);
    let forward = markers.marker_rotation(3).unwrap() * Vec3::X;
    let angle = 75_f32.to_radians();
    assert!(forward.abs_diff_eq(vec3(angle.cos(), 0., -angle.sin()), 1e-5));
    assert!(std::ptr::eq(
        markers.solid_colliders().next().unwrap(),
        &markers.colliders[0]
    ));
    assert!(std::ptr::eq(
        markers.trigger_colliders().next().unwrap(),
        &markers.colliders[1]
    ));
    for up in [
        Vec3::Y,
        Vec3::Z,
        -Vec3::Y,
        vec3(1., 2., 3.),
        Vec3::ZERO,
        Vec3::splat(f32::NAN),
    ] {
        assert_scene(&markers.with_up_axis(up), &scene.with_up_axis(up));
    }
    let model = load.take_model().unwrap();
    let pose = load.take_pose().unwrap();
    let maps = load.take_material_maps().unwrap();
    assert_eq!(model.parts[0].node, 1);
    for up in [Vec3::Y, Vec3::Z, vec3(1., 2., 3.)] {
        assert_eq!(
            markers.with_up_axis(up).rest_world(1),
            pose.with_up_axis(up).rest_world(1)
        );
    }
    drop((model, maps, pose, load));
    drop(bytes);
    assert_scene(&markers, &scene); // No source-byte/loader/model lifetime.
}

#[test]
fn independent_take_orders_are_once_only_and_do_not_change_progress() {
    let bytes = fixture::untextured_bytes();
    let expected = GltfMarkers::from_scene(&GltfScene::from_bytes(&bytes).unwrap());
    // All 24 permutations of markers, model, maps and pose.
    for a in 0..4 {
        for b in 0..4 {
            for c in 0..4 {
                for d in 0..4 {
                    let order = [a, b, c, d];
                    if (0..4).any(|i| (i + 1..4).any(|j| order[i] == order[j])) {
                        continue;
                    }
                    let mut load = loader(&bytes);
                    complete(&mut load);
                    let mut markers = None;
                    for take in order {
                        match take {
                            0 => {
                                markers = load.take_markers();
                                assert!(load.take_markers().is_none());
                            }
                            1 => {
                                assert!(load.take_model().is_some());
                                assert!(load.take_model().is_none());
                            }
                            2 => {
                                assert!(load.take_material_maps().is_some());
                                assert!(load.take_material_maps().is_none());
                            }
                            _ => {
                                assert!(load.take_pose().is_some());
                                assert!(load.take_pose().is_none());
                            }
                        }
                        assert_eq!(load.poll().unwrap().stage, GltfLoadStage::Complete);
                    }
                    drop(load);
                    assert_eq!(markers.unwrap(), expected);
                }
            }
        }
    }
}

#[test]
fn opt_in_is_composable_early_only_and_non_destructive_when_rejected() {
    let bytes = fixture::untextured_bytes();
    let mut plain = GltfModelLoader::new(&bytes, GltfLoadSettings::default()).unwrap();
    complete(&mut plain);
    assert!(plain.take_markers().is_none());
    assert!(plain.enable_markers().is_err());
    assert!(plain.take_model().is_some());
    let mut morph = GltfModelLoader::new_with_morph(&bytes, GltfLoadSettings::default()).unwrap();
    morph.enable_markers().unwrap();
    morph.enable_markers().unwrap();
    morph.poll().unwrap();
    let before = morph.progress();
    assert!(morph.enable_markers().is_err());
    assert_eq!(morph.progress(), before);
    complete(&mut morph);
    assert!(morph.take_markers().is_some());
    assert!(morph.take_model().is_some());
    assert!(morph.take_pose().is_some());
    assert!(morph.take_material_maps().is_some());
}

#[test]
fn cancel_at_every_boundary_clears_untaken_markers_and_drop_keeps_taken_data() {
    let bytes = fixture::untextured_bytes();
    let mut reference = loader(&bytes);
    let mut polls = 0;
    while reference.poll().unwrap().stage != GltfLoadStage::Complete {
        polls += 1;
    }
    for stop in 0..=polls + 1 {
        let mut load = loader(&bytes);
        for _ in 0..stop {
            load.poll().unwrap();
        }
        load.cancel();
        load.cancel();
        assert_eq!(load.progress().stage, GltfLoadStage::Cancelled);
        assert!(load.take_markers().is_none());
        assert!(load.take_model().is_none());
        assert!(load.take_pose().is_none());
        assert!(load.take_material_maps().is_none());
        assert!(load.poll().is_err());
        assert!(load.enable_markers().is_err());
    }
    let mut load = loader(&bytes);
    complete(&mut load);
    let markers = load.take_markers().unwrap();
    let pose = load.take_pose().unwrap();
    let maps = load.take_material_maps().unwrap();
    load.cancel(); // Model still owned: untaken model is released.
    assert!(load.take_model().is_none());
    drop((load, maps, pose));
    assert_eq!(markers.spawns.len(), 3);
    let mut load = loader(&bytes);
    complete(&mut load);
    let model = load.take_model().unwrap();
    load.cancel(); // Existing post-model-take no-op also preserves sidecars.
    assert_eq!(load.progress().stage, GltfLoadStage::Complete);
    let markers_after_cancel = load.take_markers().unwrap();
    assert!(load.take_pose().is_some());
    assert!(load.take_material_maps().is_some());
    drop((load, model));
    assert_eq!(markers_after_cancel, markers);
}

#[test]
fn failed_parse_or_image_preflight_never_exposes_markers() {
    let mut failed = loader(b"broken GLB");
    let error = failed.poll().unwrap_err();
    assert!(failed.take_markers().is_none());
    failed.cancel();
    assert_eq!(failed.poll().unwrap_err(), error);
    assert_eq!(failed.progress().stage, GltfLoadStage::Failed);
    let bytes = fixture::bytes();
    let settings = GltfLoadOptions::default().with_image_limits(GltfImageLimits {
        max_source_pixels: Some(0),
        max_decoded_bytes: None,
    });
    let mut failed = GltfModelLoader::new_with_markers(&bytes, settings).unwrap();
    while failed.poll().is_ok() {
        assert!(failed.take_markers().is_none());
    }
    assert_eq!(failed.progress().stage, GltfLoadStage::Failed);
    assert!(failed.take_markers().is_none());
    assert!(failed.take_model().is_none());
    assert!(failed.take_pose().is_none());
}

#[test]
fn copied_metadata_matches_invalid_worlds_and_caller_written_tag_lengths() {
    let bytes = fixture::untextured_bytes();
    let mut scene = GltfScene::from_bytes(&bytes).unwrap();
    for world in [
        Mat4::ZERO,
        Mat4::from_cols_array(&[f32::INFINITY; 16]),
        Mat4::from_scale(vec3(-2., 3., 4.)),
    ] {
        scene.nodes[0].world = world;
        let markers = GltfMarkers::from_scene(&scene);
        assert_scene(&markers, &scene);
    }
    scene.marker_tags.clear();
    assert_scene(&GltfMarkers::from_scene(&scene), &scene);
    scene.marker_tags = vec![Some("extra".into()); scene.nodes.len() + 1];
    assert_scene(&GltfMarkers::from_scene(&scene), &scene);
}
