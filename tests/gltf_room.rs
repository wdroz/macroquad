#![cfg(feature = "gltf")]
#[path = "../examples/support/room_scene.rs"]
mod room_scene;
use macroquad::{
    gltf::{GltfColliderShape, GltfScene},
    math::{vec3, Aabb, Capsule, Ray, Sphere, Vec3},
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
fn picking_checks_far_first_candidates_triggers_ties_and_closed_range() {
    let mut room = room_scene::Room::load();
    let far = GltfColliderShape::Aabb(Aabb::from_center_size(Vec3::X * 8., Vec3::ONE));
    let trigger = GltfColliderShape::Sphere(Sphere::new(Vec3::X, 0.5));
    let near = GltfColliderShape::Capsule(Capsule::new(vec3(4., -1., 0.), vec3(4., 1., 0.), 0.5));
    let miss = GltfColliderShape::Sphere(Sphere::new(Vec3::Y * 10., 1.));
    for (index, shape) in [far, trigger, near, near, miss].into_iter().enumerate() {
        room.scene.colliders[index].shape = shape;
        room.scene.colliders[index].trigger = index == 1;
    }
    room.rebuild_bounds();
    let original = room.scene.clone();
    let ray = Ray::new(Vec3::ZERO, Vec3::X * 2.);
    let near_hit = near.ray_intersect(ray.origin, ray.direction).unwrap();
    let trigger_hit = trigger.ray_intersect(ray.origin, ray.direction).unwrap();
    // Source order starts with a farther solid at index 0. Equal hit times
    // at indices 2/3 keep index 2; trigger 1 is included only by caller choice.
    assert!(far.ray_intersect(ray.origin, ray.direction).unwrap().t > near_hit.t);
    assert_eq!(room.pick(&ray, 10., false), Some((2, near_hit)));
    assert_eq!(room.pick(&ray, near_hit.t, false), Some((2, near_hit)));
    assert_eq!(room.pick(&ray, near_hit.t - 0.001, false), None);
    assert_eq!(room.pick(&ray, 10., true), Some((1, trigger_hit)));
    assert_eq!(room.pick(&ray, f32::INFINITY, false), Some((2, near_hit)));
    assert_eq!(room.scene, original);

    // An unrepresentable, distant enclosure disables the goal-query grid.
    // Complete-list picking keeps the same nearest/trigger/tie/limit policy.
    let unbounded = GltfColliderShape::Sphere(Sphere::new(Vec3::X * f32::MAX, 1.));
    assert!(unbounded.bounding_aabb().is_none());
    room.scene.colliders[4].shape = unbounded;
    room.rebuild_bounds();
    assert_eq!(room.pick(&ray, 10., false), Some((2, near_hit)));
    assert_eq!(room.pick(&ray, near_hit.t, false), Some((2, near_hit)));
    assert_eq!(room.pick(&ray, 10., true), Some((1, trigger_hit)));

    let stationary = Ray::new(Vec3::X, Vec3::ZERO);
    let inside = room.pick(&stationary, 0., true).unwrap();
    assert_eq!(inside.0, 1);
    assert_eq!(inside.1.t, 0.);
    assert_eq!(inside.1.normal, Vec3::ZERO);
    assert!(room.pick(&stationary, 0., false).is_none());
    for max_t in [-1., f32::NEG_INFINITY, f32::NAN] {
        assert!(room.pick(&ray, max_t, true).is_none());
    }
    for invalid in [
        Ray::new(Vec3::NAN, Vec3::X),
        Ray::new(Vec3::ZERO, Vec3::INFINITY),
    ] {
        assert!(room.pick(&invalid, 10., true).is_none());
    }
    for collider in &mut room.scene.colliders {
        collider.trigger = true;
    }
    assert!(room.pick(&ray, 10., false).is_none());
    assert_eq!(room.pick(&ray, 10., true), Some((1, trigger_hit)));
}

#[test]
fn picking_keeps_rounded_primitive_hits_at_the_closed_parameter_limit() {
    let mut room = room_scene::Room::load();
    let shape = GltfColliderShape::Aabb(Aabb::from_min_max(
        vec3(7., -1., -1.),
        vec3(8., 1., 1.),
    ));
    for collider in &mut room.scene.colliders {
        collider.shape = GltfColliderShape::Sphere(Sphere::new(Vec3::Y * 10., 1.));
        collider.trigger = false;
    }
    room.scene.colliders[0].shape = shape;
    room.rebuild_bounds();
    let ray = Ray::new(Vec3::ZERO, Vec3::X * 5.);
    let hit = shape.ray_intersect(ray.origin, ray.direction).unwrap();
    // The delegated f32 entry time rounds down. A grid query bounded at that
    // time would end before x = 7 in f64 and discard this valid closed-range hit.
    assert!(f64::from(ray.direction.x) * f64::from(hit.t) < 7.);
    let before = f32::from_bits(hit.t.to_bits() - 1);
    assert_eq!(room.pick(&ray, hit.t, false), Some((0, hit)));
    assert!(room.pick(&ray, before, false).is_none());

    // A corner admitted by f32 slab rounding can also be rejected by an
    // unbounded f64 grid ray. Full-list picking keeps the delegated result.
    let corner = GltfColliderShape::Aabb(Aabb::from_min_max(
        vec3(7., -1., -1.),
        vec3(8., f32::from_bits(7f32.to_bits() - 1), 1.),
    ));
    room.scene.colliders[0].shape = corner;
    room.rebuild_bounds();
    let diagonal = Ray::new(Vec3::ZERO, vec3(5., 5., 0.));
    let corner_hit = corner.ray_intersect(diagonal.origin, diagonal.direction).unwrap();
    assert_eq!(corner_hit.t, hit.t);
    assert_eq!(room.pick(&diagonal, corner_hit.t, false), Some((0, corner_hit)));
    assert!(room.pick(&diagonal, before, false).is_none());

    // Disabling the goal-query grid cannot change either picking result.
    room.scene.colliders[4].shape =
        GltfColliderShape::Sphere(Sphere::new(Vec3::X * f32::MAX, 1.));
    room.rebuild_bounds();
    assert_eq!(room.pick(&diagonal, corner_hit.t, false), Some((0, corner_hit)));
    assert!(room.pick(&diagonal, before, false).is_none());
    room.scene.colliders[0].shape = shape;
    assert_eq!(room.pick(&ray, hit.t, false), Some((0, hit)));
    assert!(room.pick(&ray, before, false).is_none());
}

#[test]
fn picking_does_not_substitute_bounding_boxes_for_exact_shapes() {
    let mut room = room_scene::Room::load();
    for collider in &mut room.scene.colliders {
        collider.shape = GltfColliderShape::Sphere(Sphere::new(Vec3::X * 4., 1.));
        collider.trigger = false;
    }
    let far = GltfColliderShape::Aabb(Aabb::from_center_size(Vec3::X * 8., Vec3::splat(2.)));
    room.scene.colliders[4].shape = far;
    room.rebuild_bounds();
    let ray = Ray::new(vec3(0., 0.9, 0.9), Vec3::X * 2.);
    let sphere = room.scene.colliders[0].shape;
    assert!(sphere
        .bounding_aabb()
        .unwrap()
        .ray_intersect(ray.origin, ray.direction)
        .is_some());
    assert!(sphere.ray_intersect(ray.origin, ray.direction).is_none());
    assert_eq!(
        room.pick(&ray, 10., false),
        Some((4, far.ray_intersect(ray.origin, ray.direction).unwrap()))
    );
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
