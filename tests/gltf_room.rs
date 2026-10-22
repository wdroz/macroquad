#![cfg(feature = "gltf")]
#[path = "../examples/support/room_scene.rs"]
mod room_scene;
use macroquad::{
    gltf::{GltfColliderShape, GltfScene},
    math::{vec3, Sphere, Vec3},
};
use serde_json::Value;

#[test]
fn committed_room_contract_and_axis_conversion() {
    let room = room_scene::Room::load();
    assert_eq!(
        room.scene.nodes.iter().filter(|n| n.mesh.is_some()).count(),
        4
    );
    assert!(room_scene::BYTES.len() < 16 * 1024);
    assert!(room.scene.images.is_empty());
    let z = room.scene.clone().with_up_axis(Vec3::Z);
    assert_eq!(z.marker_tags, room.scene.marker_tags);
    assert!((z.spawns[0].position.z - 0.4).abs() < 1e-5);
}

#[test]
fn authored_walls_block_and_goal_does_not() {
    let room = room_scene::Room::load();
    let end = room.step(room.spawn, vec3(20., 0., 0.));
    assert!(end.x > 3.4 && end.x < 3.51, "{end:?}");
    assert!(!room.reached(room.spawn));
    let end = room.step(room.spawn, vec3(0., 0., -4.));
    assert!(room.reached(end));
    assert!((end.z + 2.).abs() < 1e-4);
    let back = room.step(end, vec3(0., 0., -20.));
    assert!(back.z > -3.51 && back.z < -3.4, "{back:?}");
}

#[test]
fn goal_bounds_rebuild_and_complete_candidate_fallback_keep_exact_queries() {
    let mut room = room_scene::Room::load();
    let goal_index = room
        .scene
        .colliders
        .iter()
        .position(|collider| room.scene.marker_tag(collider.node) == Some("goal"))
        .unwrap();
    let moved = Sphere::new(vec3(12., 0.4, -2.), 0.6);
    room.scene.colliders[goal_index].shape = GltfColliderShape::Sphere(moved);
    // The caller refreshes snapshots after changing world-space geometry.
    room.rebuild_bounds();
    assert!(!room.reached(room.goal.center));
    assert!(room.reached(moved.center));

    // All indexed shapes have finite bounds, but this query's positive radius
    // extends beyond f32::MAX. Complete candidates must be used for this query
    // even while the grid itself is available.
    let far_point = GltfColliderShape::Sphere(Sphere::new(Vec3::X * f32::MAX, 0.));
    room.scene.colliders[goal_index].shape = far_point;
    assert!(room
        .scene
        .colliders
        .iter()
        .all(|collider| collider.shape.bounding_aabb().is_some()));
    room.rebuild_bounds();
    let far_query = Sphere::new(Vec3::X * f32::MAX, room_scene::RADIUS);
    assert!(far_query.bounding_aabb().is_none());
    assert!(far_point.overlaps_sphere(&far_query));
    assert!(room.reached(far_query.center));

    // Valid finite sphere inputs can have no finite bounding enclosure. The
    // candidate fallback must still run the original shape query unchanged.
    let huge = GltfColliderShape::Sphere(Sphere::new(Vec3::X * f32::MAX, f32::MAX));
    assert!(huge.bounding_aabb().is_none());
    let query = Sphere::new(Vec3::ZERO, room_scene::RADIUS);
    assert!(huge.overlaps_sphere(&query));
    room.scene.colliders[goal_index].shape = huge;
    room.rebuild_bounds();
    assert_eq!(room.reached(query.center), huge.overlaps_sphere(&query));

    // Rebuilding a representable snapshot restores normal grid queries.
    room.scene.colliders[goal_index].shape = GltfColliderShape::Sphere(moved);
    room.rebuild_bounds();
    assert!(room.reached(moved.center));
    assert!(!room.reached(Vec3::ZERO));
}

#[test]
fn exported_duplicate_names_work_with_suffix_fallback_and_require_empty_shapes() {
    // Read the committed GLB JSON chunk. No generator/Blender invocation.
    let bytes = room_scene::BYTES;
    let length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let mut doc: Value = serde_json::from_slice(&bytes[20..20 + length]).unwrap();
    // Meshes are irrelevant for this marker-only derivative. Remove references
    // so the JSON document has no external binary buffer dependency.
    for key in ["buffers", "bufferViews", "accessors", "meshes", "materials"] {
        doc.as_object_mut().unwrap().remove(key);
    }
    for node in doc["nodes"].as_array_mut().unwrap() {
        node.as_object_mut().unwrap().remove("mesh");
        if let Some(extras) = node.get_mut("extras") {
            extras.as_object_mut().unwrap().remove("mq_role");
        }
    }
    let fallback = GltfScene::from_bytes(&serde_json::to_vec(&doc).unwrap()).unwrap();
    assert_eq!(fallback.colliders.len(), 5);
    assert_eq!(fallback.spawns.len(), 1);
    assert_eq!(fallback.spawns[0].name.as_deref(), Some("player_spawn.001"));
    assert_eq!(fallback.marker_tag(fallback.spawns[0].node), Some("player"));
    for node in doc["nodes"].as_array_mut().unwrap() {
        if let Some(extras) = node.get_mut("extras") {
            extras.as_object_mut().unwrap().remove("mq_shape");
        }
    }
    let unshaped = GltfScene::from_bytes(&serde_json::to_vec(&doc).unwrap()).unwrap();
    assert!(unshaped.colliders.is_empty());
    assert_eq!(unshaped.spawns.len(), 1);
}
