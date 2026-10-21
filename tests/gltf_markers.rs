#![cfg(feature = "gltf")]

use macroquad::gltf::{GltfCollider, GltfColliderShape, GltfScene, GltfSpawn};
use macroquad::math::{vec3, Aabb, Vec3};
use serde_json::{json, Value};

fn scene(nodes: Vec<Value>) -> GltfScene {
    let roots: Vec<usize> = (0..nodes.len()).collect();
    let document = json!({
        "asset": {"version": "2.0"},
        "scenes": [{"nodes": roots}],
        "nodes": nodes,
    });
    GltfScene::from_bytes(&serde_json::to_vec(&document).unwrap()).unwrap()
}

#[test]
fn suffix_only_blender_duplicate_is_a_spawn_with_its_original_name() {
    let scene = scene(vec![json!({
        "name": "patrol_spawn.001",
        "translation": [3, 4, 5],
    })]);
    assert_eq!(scene.spawns.len(), 1);
    assert!(scene.colliders.is_empty());
    assert_eq!(scene.spawns[0].name.as_deref(), Some("patrol_spawn.001"));
    assert_eq!(scene.spawns[0].position, vec3(3., 4., 5.));
    assert_eq!(scene.marker_tag(scene.spawns[0].node), None);
}

#[test]
fn every_role_accepts_one_optional_ascii_digit_tail_without_a_number_limit() {
    for tail in ["", ".0", ".001", ".123456789012345678901234567890"] {
        let scene = scene(vec![
            json!({"name": format!("wall_collider{tail}"), "extras": {"mq_shape": "box"}}),
            json!({"name": format!("bell_trigger{tail}"), "extras": {"mq_shape": "sphere"}}),
            json!({"name": format!("patrol_spawn{tail}")}),
        ]);
        assert_eq!(scene.colliders.len(), 2, "{tail}");
        assert!(!scene.colliders[0].trigger);
        assert!(scene.colliders[1].trigger);
        assert!(matches!(
            scene.colliders[0].shape,
            GltfColliderShape::Aabb(_)
        ));
        assert!(matches!(
            scene.colliders[1].shape,
            GltfColliderShape::Sphere(_)
        ));
        assert_eq!(scene.spawns.len(), 1, "{tail}");
        assert_eq!(scene.spawns[0].node, 2);
    }
}

#[test]
fn suffix_matching_rejects_case_changes_extra_tails_and_non_ascii_digits() {
    for role in ["collider", "trigger", "spawn"] {
        for name in [
            format!("patrol_{}", role.to_uppercase()),
            format!("patrol_{}.001", role.to_uppercase()),
            format!("patrol_{role}."),
            format!("patrol_{role}..001"),
            format!("patrol_{role}.001.002"),
            format!("patrol_{role}.001x"),
            format!("patrol_{role}.+001"),
            format!("patrol_{role}.-001"),
            format!("patrol_{role}.1e3"),
            format!("patrol_{role}.００１"),
            format!("patrol_{role}.١٢٣"),
            format!("patrol_{role}.001 "),
            format!("patrol_{role}Extra.001"),
            format!("patrol_{role} .001"),
            format!("patrol{role}.001"),
        ] {
            let scene = scene(vec![json!({"name": name, "extras": {"mq_shape": "box"}})]);
            assert!(scene.spawns.is_empty(), "{name}");
            assert!(scene.colliders.is_empty(), "{name}");
        }
    }
}

#[test]
fn valid_extras_roles_override_duplicate_names_and_invalid_roles_keep_fallback() {
    for suffix in ["collider", "trigger", "spawn"] {
        for role in ["collider", "trigger", "spawn"] {
            let scene = scene(vec![json!({
                "name": format!("pad_{suffix}.001"),
                "extras": {"mq_role": role, "mq_shape": "box", "mq_tag": "pad"},
            })]);
            assert_eq!(scene.spawns.len(), usize::from(role == "spawn"));
            assert_eq!(scene.colliders.len(), usize::from(role != "spawn"));
            if let Some(collider) = scene.colliders.first() {
                assert_eq!(collider.trigger, role == "trigger");
            }
            assert_eq!(scene.marker_tag(0), Some("pad"));
        }
    }
    for invalid in [Value::Null, json!(1), json!("Spawn"), json!("other")] {
        let scene = scene(vec![json!({
            "name": "patrol_spawn.001", "extras": {"mq_role": invalid},
        })]);
        assert_eq!(scene.spawns.len(), 1);
    }
}

#[test]
fn tags_select_spawns_and_keep_strings_without_changing_marker_roles() {
    let scene = scene(vec![
        json!({"name": "patrol_spawn.001", "extras": {"mq_tag": "guard"}}),
        json!({"name": "patrol_spawn.002", "extras": {"mq_tag": "player"}}),
        json!({"name": "wall_collider.001", "extras": {"mq_shape": "box", "mq_tag": "wall"}}),
        json!({"name": "bell_trigger.001", "extras": {"mq_shape": "sphere", "mq_tag": "bell"}}),
        json!({"name": "decoration", "extras": {"mq_tag": "  Guard  "}}),
        json!({"name": "empty_spawn.001", "extras": {"mq_tag": ""}}),
        json!({"name": "missing_shape_collider.001", "extras": {"mq_tag": "unshaped"}}),
    ]);
    let player = scene
        .spawns
        .iter()
        .find(|spawn| scene.marker_tag(spawn.node) == Some("player"))
        .unwrap();
    assert_eq!(player.name.as_deref(), Some("patrol_spawn.002"));
    assert_eq!(scene.spawns.len(), 3);
    assert_eq!(scene.colliders.len(), 2);
    for (node, tag) in [
        "guard",
        "player",
        "wall",
        "bell",
        "  Guard  ",
        "",
        "unshaped",
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(scene.marker_tag(node), Some(*tag));
        assert_eq!(scene.marker_tags[node].as_deref(), Some(*tag));
    }
}

#[test]
fn missing_non_string_nested_or_non_object_tags_are_absent() {
    let scene = scene(vec![
        json!({"name": "plain_spawn"}),
        json!({"extras": {}}),
        json!({"extras": {"mq_tag": null}}),
        json!({"extras": {"mq_tag": 42}}),
        json!({"extras": {"mq_tag": true}}),
        json!({"extras": {"mq_tag": ["guard"]}}),
        json!({"extras": {"mq_tag": {"text": "guard"}}}),
        json!({"extras": {"nested": {"mq_tag": "guard"}}}),
        json!({"extras": {"MQ_TAG": "guard"}}),
        json!({"extras": "guard"}),
        json!({"extras": [{"mq_tag": "guard"}]}),
    ]);
    assert_eq!(scene.marker_tags, vec![None; scene.nodes.len()]);
    for node in 0..=scene.nodes.len() {
        assert_eq!(scene.marker_tag(node), None);
    }
    assert_eq!(scene.marker_tag(usize::MAX), None);
}

#[test]
fn tags_follow_loaded_preorder_and_survive_clone_and_axis_conversion() {
    let scene = GltfScene::from_bytes(
        br#"{
        "asset":{"version":"2.0"}, "scene":1,
        "scenes":[{"nodes":[0]},{"nodes":[3]}],
        "nodes":[
            {"name":"unused_spawn","extras":{"mq_tag":"unused"}},
            {"name":"child_spawn.001","extras":{"mq_tag":"child"}},
            {"name":"sibling_spawn.001"},
            {"name":"parent","children":[1,2],"translation":[0,3,0],"extras":{"mq_tag":"parent"}}
        ]
    }"#,
    )
    .unwrap();
    assert_eq!(
        scene.marker_tags,
        vec![Some("parent".into()), Some("child".into()), None]
    );
    assert_eq!(scene.spawns[0].node, 1);
    assert_eq!(scene.marker_tag(scene.spawns[0].node), Some("child"));
    assert_eq!(scene.marker_tag(3), None);
    assert_eq!(scene.clone(), scene);
    let z_up = scene.with_up_axis(Vec3::Z);
    assert_eq!(z_up.marker_tags, scene.marker_tags);
    assert_eq!(z_up.marker_tag(z_up.spawns[0].node), Some("child"));
    assert!((z_up.spawns[0].position - vec3(0., 0., 3.)).length() < 1e-5);
}

#[test]
fn tags_decode_unicode_json_escapes_and_surrogate_pairs() {
    let scene = GltfScene::from_bytes(
        br#"{
        "asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],
        "nodes":[{"name":"patrol_spawn.001","extras":{
            "note":"\ud83d\udd14", "mq_tag":"caf\u00e9\t\"guard\"\n\ud83d\udd14"
        }}]
    }"#,
    )
    .unwrap();
    assert_eq!(scene.marker_tag(0), Some("café\t\"guard\"\n🔔"));
}

#[test]
fn escaped_unicode_tags_before_roles_do_not_hide_extras_precedence() {
    let scene = GltfScene::from_bytes(
        br#"{
        "asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],
        "nodes":[{"name":"patrol_spawn.001","extras":{
            "mq_tag":"\ud83d\udd14", "mq_role":"trigger", "mq_shape":"box"
        }}]
    }"#,
    )
    .unwrap();
    assert!(scene.spawns.is_empty());
    assert_eq!(scene.colliders.len(), 1);
    assert!(scene.colliders[0].trigger);
    assert_eq!(scene.marker_tag(scene.colliders[0].node), Some("🔔"));
}

#[test]
fn undecodable_unrelated_strings_do_not_hide_later_marker_fields() {
    // Raw glTF extras may retain an unpaired JSON UTF-16 escape even though
    // it cannot become a Rust string. Only the selected field needs decoding.
    for escape in [r"\ud800", r"\udfff", r"\ud800x", r"\ud800\u0041"] {
        let document = format!(
            r#"{{"asset":{{"version":"2.0"}},"scenes":[{{"nodes":[0]}}],
            "nodes":[{{"name":"patrol_spawn.001","extras":{{
                "note":"{escape}", "mq_tag":"guard", "mq_role":"trigger", "mq_shape":"box"
            }}}}]}}"#
        );
        let scene = GltfScene::from_bytes(document.as_bytes()).unwrap();
        assert_eq!(scene.marker_tag(0), Some("guard"), "{escape}");
        assert!(scene.spawns.is_empty(), "{escape}");
        assert_eq!(scene.colliders.len(), 1, "{escape}");
        assert!(scene.colliders[0].trigger, "{escape}");
    }
}

#[test]
fn undecodable_tag_is_absent_without_changing_explicit_role() {
    let scene = GltfScene::from_bytes(
        br#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],
        "nodes":[{"name":"patrol_spawn.001","extras":{
            "mq_tag":"\ud800", "mq_role":"trigger", "mq_shape":"box"
        }}]}"#,
    )
    .unwrap();
    assert_eq!(scene.marker_tag(0), None);
    assert!(scene.spawns.is_empty());
    assert_eq!(scene.colliders.len(), 1);
    assert!(scene.colliders[0].trigger);
}

#[test]
fn undecodable_keys_and_nested_metadata_do_not_hide_later_marker_fields() {
    let scene = GltfScene::from_bytes(
        br#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],
        "nodes":[{"name":"patrol_spawn.001","extras":{
            "\ud800":"ignored",
            "nested":[{"mq_tag":"hidden","text":"\udfff","brackets":"[{\"}]"}],
            "mq\u005ftag":"visible", "mq\u005frole":"trigger", "mq_shape":"box"
        }}]}"#,
    )
    .unwrap();
    assert_eq!(scene.marker_tag(0), Some("visible"));
    assert!(scene.spawns.is_empty());
    assert_eq!(scene.colliders.len(), 1);
    assert!(scene.colliders[0].trigger);
}

#[test]
fn scenes_without_a_selected_scene_have_no_tag_metadata() {
    let scene = GltfScene::from_bytes(
        br#"{
        "asset":{"version":"2.0"},
        "nodes":[{"name":"unused_spawn.001","extras":{"mq_tag":"unused"}}]
    }"#,
    )
    .unwrap();
    assert!(scene.nodes.is_empty());
    assert!(scene.marker_tags.is_empty());
    assert_eq!(scene.marker_tag(0), None);
}

#[test]
fn existing_marker_literals_remain_unchanged_and_metadata_lookup_is_checked() {
    let spawn = GltfSpawn {
        node: 0,
        name: None,
        position: Vec3::ZERO,
    };
    let collider = GltfCollider {
        node: 0,
        name: None,
        trigger: false,
        shape: GltfColliderShape::Aabb(Aabb::from_center_size(Vec3::ZERO, Vec3::ONE)),
    };
    assert_eq!(spawn.node, collider.node);
    let mut scene = scene(vec![json!({"extras": {"mq_tag": "guard"}})]);
    scene.nodes.clear();
    assert_eq!(scene.marker_tag(0), None);
    let mut scene = self::scene(vec![json!({"extras": {"mq_tag": "guard"}})]);
    scene.marker_tags.clear();
    assert_eq!(scene.marker_tag(0), None);
}
