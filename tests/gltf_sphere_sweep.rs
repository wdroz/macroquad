#![cfg(feature = "gltf")]

use macroquad::prelude::*;
use serde_json::json;

fn shapes() -> [GltfColliderShape; 4] {
    [
        GltfColliderShape::Aabb(Aabb::from_center_size(Vec3::ZERO, Vec3::splat(2.0))),
        GltfColliderShape::Obb(Obb::new(
            Vec3::ZERO,
            vec3(1.0, 0.7, 1.2),
            Quat::from_euler(EulerRot::XYZ, 0.37, 0.53, -0.21),
        )),
        GltfColliderShape::Sphere(Sphere::new(Vec3::ZERO, 1.0)),
        GltfColliderShape::Capsule(Capsule::new(-Vec3::Y, Vec3::Y, 0.6)),
    ]
}

fn primitive(shape: GltfColliderShape, body: &Sphere, delta: Vec3) -> Option<SweepHit> {
    match shape {
        GltfColliderShape::Aabb(shape) => body.sweep_aabb(delta, &shape),
        GltfColliderShape::Obb(shape) => body.sweep_obb(delta, &shape),
        GltfColliderShape::Sphere(shape) => body.sweep_sphere(delta, &shape),
        GltfColliderShape::Capsule(shape) => body.sweep_capsule(delta, &shape),
    }
}

fn bits(hit: Option<SweepHit>) -> Option<[u32; 7]> {
    hit.map(|hit| {
        [
            hit.t.to_bits(),
            hit.normal.x.to_bits(),
            hit.normal.y.to_bits(),
            hit.normal.z.to_bits(),
            hit.point.x.to_bits(),
            hit.point.y.to_bits(),
            hit.point.z.to_bits(),
        ]
    })
}

fn assert_parity(shape: GltfColliderShape, body: Sphere, delta: Vec3) -> Option<SweepHit> {
    let actual = shape.sweep_sphere(&body, delta);
    assert_eq!(
        bits(actual),
        bits(primitive(shape, &body, delta)),
        "shape={shape:?}, body={body:?}, delta={delta:?}"
    );
    actual
}

#[test]
fn all_variants_preserve_complete_hits_and_misses_bit_for_bit() {
    let body = Sphere::new(vec3(-4.0, 0.0, 0.0), 0.4);
    for shape in shapes() {
        let hit = assert_parity(shape, body, Vec3::X * 8.0).unwrap();
        assert!(hit.t > 0.0 && hit.t < 1.0);
        assert!(hit.normal.dot(Vec3::X) < 0.0);
        assert!(hit.point.is_finite());
        assert!(assert_parity(shape, body.offset(Vec3::Y * 8.0), Vec3::X * 8.0).is_none());

        // Preserve each primitive's oblique/edge points and initial tie choices.
        for center in [
            Vec3::ZERO,
            vec3(-4.0, 1.2, 0.4),
            vec3(4.0, -1.2, -0.4),
            vec3(-2.0, -2.0, -2.0),
        ] {
            for radius in [0.0, 0.4, 1.2] {
                for delta in [Vec3::ZERO, -center * 2.0, Vec3::ONE, -Vec3::ONE] {
                    assert_parity(shape, Sphere::new(center, radius), delta);
                }
            }
        }
    }
}

#[test]
fn touching_shallow_and_initial_contacts_keep_primitive_motion_rules() {
    // The left surface is x = -1 for each variant, including the rotated OBB.
    let shapes = [
        shapes()[0],
        GltfColliderShape::Obb(Obb::new(Vec3::ZERO, Vec3::ONE, Quat::from_rotation_x(0.37))),
        shapes()[2],
        GltfColliderShape::Capsule(Capsule::new(-Vec3::Y, Vec3::Y, 1.0)),
    ];
    let touching = Sphere::new(vec3(-1.25, 0.0, 0.0), 0.25);
    for shape in shapes {
        for body in [touching, touching.offset(Vec3::X * 0.00005)] {
            assert_eq!(assert_parity(shape, body, Vec3::X).unwrap().t, 0.0);
            for delta in [Vec3::ZERO, -Vec3::X, Vec3::Y, Vec3::Z] {
                assert!(assert_parity(shape, body, delta).is_none());
            }
        }
        let overlapping = touching.offset(Vec3::X * 0.001);
        for delta in [Vec3::ZERO, Vec3::X, -Vec3::X] {
            assert_eq!(assert_parity(shape, overlapping, delta).unwrap().t, 0.0);
        }
    }
}

#[test]
fn degenerate_static_shapes_and_endpoint_hits_are_not_filtered() {
    let shapes = [
        GltfColliderShape::Aabb(Aabb::from_center_size(Vec3::ZERO, Vec3::ZERO)),
        GltfColliderShape::Obb(Obb::new(Vec3::ZERO, Vec3::ZERO, Quat::IDENTITY)),
        GltfColliderShape::Sphere(Sphere::new(Vec3::ZERO, 0.0)),
        GltfColliderShape::Capsule(Capsule::new(Vec3::ZERO, Vec3::ZERO, 0.0)),
    ];
    let body = Sphere::new(vec3(-3.0, 0.0, 0.0), 0.5);
    for shape in shapes {
        let hit = assert_parity(shape, body, Vec3::X * 2.5).unwrap();
        assert_eq!(hit.t, 1.0);
        assert_eq!(hit.normal, -Vec3::X);
        assert_eq!(hit.point, Vec3::ZERO);
        assert!(assert_parity(shape, body, Vec3::ZERO).is_none());
    }
}

#[test]
fn invalid_bodies_deltas_and_shapes_keep_delegated_behavior() {
    let body = Sphere::new(vec3(-3.0, 0.0, 0.0), 0.5);
    for shape in shapes() {
        for invalid in [
            Sphere::new(body.center, -1.0),
            Sphere::new(Vec3::NAN, 0.5),
            Sphere::new(vec3(0.0, f32::INFINITY, 0.0), 0.5),
            Sphere::new(body.center, f32::NAN),
            Sphere::new(body.center, f32::INFINITY),
        ] {
            assert!(assert_parity(shape, invalid, Vec3::X * 6.0).is_none());
        }
        for delta in [
            Vec3::NAN,
            vec3(f32::INFINITY, 0.0, 0.0),
            vec3(0.0, 0.0, f32::NEG_INFINITY),
        ] {
            assert!(assert_parity(shape, body, delta).is_none());
        }
        // Do not impose an adapter clamp on finite-precision limits.
        assert_parity(shape, body, Vec3::X * f32::MAX);
    }
    for shape in [
        GltfColliderShape::Aabb(Aabb {
            min: Vec3::ONE,
            max: -Vec3::ONE,
        }),
        GltfColliderShape::Aabb(Aabb {
            min: Vec3::NAN,
            max: Vec3::ONE,
        }),
        GltfColliderShape::Obb(Obb::new(Vec3::ZERO, -Vec3::ONE, Quat::IDENTITY)),
        GltfColliderShape::Obb(Obb::new(
            Vec3::ZERO,
            Vec3::ONE,
            Quat::from_xyzw(0.0, 0.0, 0.0, 0.0),
        )),
        GltfColliderShape::Obb(Obb::new(
            Vec3::ZERO,
            Vec3::ONE,
            Quat::from_xyzw(f32::NAN, 0.0, 0.0, 1.0),
        )),
        GltfColliderShape::Sphere(Sphere::new(Vec3::ZERO, -1.0)),
        GltfColliderShape::Sphere(Sphere::new(Vec3::NAN, 1.0)),
        GltfColliderShape::Sphere(Sphere::new(Vec3::ZERO, f32::INFINITY)),
        GltfColliderShape::Capsule(Capsule::new(Vec3::ZERO, Vec3::Y, -1.0)),
        GltfColliderShape::Capsule(Capsule::new(Vec3::ZERO, Vec3::NAN, 1.0)),
        GltfColliderShape::Capsule(Capsule::new(Vec3::ZERO, Vec3::Y, f32::NAN)),
    ] {
        assert!(assert_parity(shape, body, Vec3::X * 6.0).is_none());
    }
}

fn scene() -> GltfScene {
    GltfScene::from_bytes(&serde_json::to_vec(&json!({
        "asset": {"version": "2.0"}, "scenes": [{"nodes": [0]}],
        "nodes": [
            {"name": "root", "translation": [1, 2, 3], "children": [1, 2, 3, 4]},
            {"name": "box_collider", "extras": {"mq_shape": "box"}},
            {"name": "rotated_collider", "translation": [4, 0, 0],
             "rotation": [0, 0.38268343, 0, 0.92387953], "extras": {"mq_shape": "box"}},
            {"name": "orb_collider", "translation": [8, 0, 0], "extras": {"mq_shape": "sphere"}},
            {"name": "post_trigger", "translation": [12, 0, 0], "scale": [0.5, 2, 0.5],
             "extras": {"mq_shape": "capsule"}}
        ]
    })).unwrap()).unwrap()
}

fn check_world_shape(shape: GltfColliderShape) {
    let center = match shape {
        GltfColliderShape::Aabb(shape) => shape.center(),
        GltfColliderShape::Obb(shape) => shape.center,
        GltfColliderShape::Sphere(shape) => shape.center,
        GltfColliderShape::Capsule(shape) => (shape.a + shape.b) * 0.5,
    };
    for axis in [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z] {
        let body = Sphere::new(center - axis * 8.0, 0.4);
        let hit = assert_parity(shape, body, axis * 16.0).unwrap();
        assert!(hit.t > 0.0 && hit.t < 1.0);
        assert!(hit.normal.dot(axis) < 0.0);
    }
}

#[test]
fn hierarchy_shapes_are_queried_at_their_current_static_snapshot() {
    let scene = scene();
    assert!(matches!(
        scene.colliders[0].shape,
        GltfColliderShape::Aabb(_)
    ));
    assert!(matches!(
        scene.colliders[1].shape,
        GltfColliderShape::Obb(_)
    ));
    assert!(matches!(
        scene.colliders[2].shape,
        GltfColliderShape::Sphere(_)
    ));
    assert!(matches!(
        scene.colliders[3].shape,
        GltfColliderShape::Capsule(_)
    ));
    let original = scene.clone();
    let mut pose = GltfModelPose::from_scene(&scene).hierarchy().unwrap();
    for rotation in [Quat::IDENTITY, Quat::from_rotation_z(0.37)] {
        pose.set_local_matrix(
            pose.handle(0).unwrap(),
            Mat4::from_scale_rotation_translation(
                vec3(1.2, 0.8, 1.1),
                rotation,
                vec3(-4.0, 3.0, 7.0),
            ),
        )
        .unwrap();
        for collider in &scene.colliders {
            let snapshot = pose.collider_shape(&scene, collider).unwrap();
            assert_ne!(snapshot, collider.shape);
            check_world_shape(snapshot);
            assert_eq!(pose.collider_shape(&scene, collider), Some(snapshot));
        }
    }
    assert_eq!(scene, original);
}

#[test]
fn converted_and_posed_non_y_up_scenes_use_world_space_spheres() {
    for up in [Vec3::Z, Vec3::X, vec3(1.0, 2.0, 3.0).normalize()] {
        let scene = scene().with_up_axis(up);
        let GltfColliderShape::Capsule(capsule) = scene.colliders[3].shape else {
            panic!("capsule must retain its shape");
        };
        assert!((capsule.b - capsule.a).normalize().dot(up) > 0.9999);
        let mut pose = GltfModelPose::from_scene(&scene).hierarchy().unwrap();
        let root = pose.handle(0).unwrap();
        let shifted = Mat4::from_translation(vec3(2.0, -1.0, 4.0)) * pose.world(root).unwrap();
        pose.set_local_matrix(root, shifted).unwrap();
        for collider in &scene.colliders {
            check_world_shape(collider.shape);
            check_world_shape(pose.collider_shape(&scene, collider).unwrap());
        }
    }
}

// Exercise the maintained asset-free recipe as well as the public dispatch.
#[path = "../examples/gltf_sphere_sweep.rs"]
mod recipe;

#[test]
fn recipe_selects_earliest_solid_and_only_reachable_tagged_triggers() {
    recipe::run();
}
