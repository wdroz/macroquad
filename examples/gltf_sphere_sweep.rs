//! Asset-free, headless glTF sphere queries:
//! `cargo run --features gltf --example gltf_sphere_sweep`.
//!
//! The caller chooses a first solid, stops at it, then explicitly checks only
//! tagged triggers along the permitted path. Queries do not apply response or
//! remember enter/exit events. No window, GPU, Blender or external asset is needed.

#[cfg(feature = "gltf")]
use macroquad::prelude::*;

#[cfg(feature = "gltf")]
fn first_solid_hit(scene: &GltfScene, body: &Sphere, delta: Vec3) -> Option<(usize, SweepHit)> {
    let mut first: Option<(usize, SweepHit)> = None;
    for collider in scene.solid_colliders() {
        let Some(hit) = collider.shape.sweep_sphere(body, delta) else {
            continue;
        };
        // Equal-time hits keep the earlier scene candidate: caller-owned policy.
        if first.is_none_or(|(_, previous)| hit.t < previous.t) {
            first = Some((collider.node, hit));
        }
    }
    first
}

#[cfg(feature = "gltf")]
pub fn run() {
    // Empty nodes need explicit shapes. Source order puts the far solid first,
    // so taking the first returned collider would produce the wrong stop.
    let scene = GltfScene::from_bytes(
        br#"{
          "asset": {"version": "2.0"}, "scenes": [{"nodes": [0, 1, 2, 3]}],
          "nodes": [
            {"name": "far_collider", "translation": [5, 0, 0],
             "extras": {"mq_shape": "box"}},
            {"name": "gate_trigger", "scale": [0.25, 0.25, 0.25],
             "extras": {"mq_shape": "sphere", "mq_tag": "checkpoint"}},
            {"name": "near_collider", "translation": [2, 0, 0], "scale": [0.5, 0.5, 0.5],
             "extras": {"mq_shape": "capsule"}},
            {"name": "later_trigger", "translation": [4, 0, 0], "scale": [0.25, 0.25, 0.25],
             "extras": {"mq_shape": "sphere", "mq_tag": "checkpoint"}}
          ]
        }"#,
    )
    .expect("tiny synthetic marker scene");
    assert_eq!(scene.solid_colliders().count(), 2);
    assert_eq!(scene.trigger_colliders().count(), 2);

    let body = Sphere::new(vec3(-3.0, 0.0, 0.0), 0.25);
    let delta = Vec3::X * 10.0;
    let (solid_node, hit) = first_solid_hit(&scene, &body, delta).unwrap();
    assert_eq!(solid_node, 2);
    assert!((hit.t - 0.425).abs() < 1e-6);
    assert_eq!(hit.normal, -Vec3::X);

    // This example stops the sphere exactly at the earliest solid. A game can
    // choose its own response and trigger path, including a slide, instead.
    let permitted_delta = delta * hit.t;
    let mut reached = Vec::new();
    for trigger in scene.trigger_colliders() {
        if scene.marker_tag(trigger.node) != Some("checkpoint") {
            continue;
        }
        // Overlap counts a resting touch too, which an outward/tangent sweep
        // intentionally omits. This is an explicit application policy.
        if trigger.shape.overlaps_sphere(&body)
            || trigger.shape.sweep_sphere(&body, permitted_delta).is_some()
        {
            reached.push(trigger.node);
        }
    }
    assert_eq!(reached, [1]);
    let gate = scene.trigger_colliders().next().unwrap();
    assert!(!gate.shape.overlaps_sphere(&body));
    assert!(!gate.shape.overlaps_sphere(&body.offset(permitted_delta)));
    // The swept query catches the gate although neither endpoint overlaps it.
    assert!(gate.shape.sweep_sphere(&body, permitted_delta).is_some());
    let later = scene.trigger_colliders().nth(1).unwrap();
    assert!(later.shape.sweep_sphere(&body, delta).unwrap().t > hit.t);
    assert!(later.shape.sweep_sphere(&body, permitted_delta).is_none());

    let stopped = body.offset(permitted_delta);
    assert!((stopped.center.x - 1.25).abs() < 1e-6);
    println!(
        "GLTF_SPHERE_SWEEP_OK solid_node={solid_node} t={} reached_trigger_nodes={reached:?} center={:?}",
        hit.t, stopped.center
    );
}

#[cfg(all(not(test), feature = "gltf"))]
fn main() {
    run();
}

#[cfg(all(not(test), not(feature = "gltf")))]
fn main() {
    eprintln!("Run with --features gltf");
}
