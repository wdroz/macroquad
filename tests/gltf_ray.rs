#![cfg(feature = "gltf")]

use macroquad::prelude::*;
use serde_json::json;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct CountingAllocator;
thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.with(|count| {
            if let Some(value) = count.get() {
                count.set(Some(value + 1));
            }
        });
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        System.dealloc(pointer, layout)
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

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

fn primitive(shape: GltfColliderShape, origin: Vec3, direction: Vec3) -> Option<RayHit> {
    match shape {
        GltfColliderShape::Aabb(shape) => shape.ray_intersect(origin, direction),
        GltfColliderShape::Obb(shape) => shape.ray_intersect(origin, direction),
        GltfColliderShape::Sphere(shape) => shape.ray_intersect(origin, direction),
        GltfColliderShape::Capsule(shape) => shape.ray_intersect(origin, direction),
    }
}

fn bits(hit: Option<RayHit>) -> Option<[u32; 7]> {
    hit.map(|hit| {
        [
            hit.t.to_bits(),
            hit.point.x.to_bits(),
            hit.point.y.to_bits(),
            hit.point.z.to_bits(),
            hit.normal.x.to_bits(),
            hit.normal.y.to_bits(),
            hit.normal.z.to_bits(),
        ]
    })
}

fn assert_parity(shape: GltfColliderShape, origin: Vec3, direction: Vec3) -> Option<RayHit> {
    let actual = shape.ray_intersect(origin, direction);
    assert_eq!(
        bits(actual),
        bits(primitive(shape, origin, direction)),
        "shape={shape:?}, origin={origin:?}, direction={direction:?}"
    );
    actual
}

#[test]
fn all_variants_preserve_hits_misses_and_nonunit_directions_bit_for_bit() {
    for shape in shapes() {
        for speed in [0.25, 1.0, 2.0, 17.0] {
            let hit = assert_parity(shape, -Vec3::X * 4.0, Vec3::X * speed).unwrap();
            assert!(hit.t > 0.0);
            assert!(hit.normal.dot(Vec3::X) < 0.0);
            assert!(assert_parity(shape, vec3(-4.0, 8.0, 0.0), Vec3::X * speed).is_none());
            assert!(assert_parity(shape, -Vec3::X * 4.0, -Vec3::X * speed).is_none());
        }
        for origin in [Vec3::ZERO, vec3(-4.0, 1.2, 0.4), vec3(4.0, -1.2, -0.4)] {
            for direction in [Vec3::ZERO, Vec3::ONE, -Vec3::ONE, -origin, Vec3::X * 1e-30] {
                assert_parity(shape, origin, direction);
            }
        }
    }
}

#[test]
fn inside_boundary_and_zero_directions_keep_each_primitive_normal() {
    // Every variant has the exact left face/barrel at x = -1.
    let aligned = [
        shapes()[0],
        GltfColliderShape::Obb(Obb::new(Vec3::ZERO, Vec3::ONE, Quat::IDENTITY)),
        shapes()[2],
        GltfColliderShape::Capsule(Capsule::new(-Vec3::Y, Vec3::Y, 1.0)),
    ];
    for shape in aligned {
        for direction in [Vec3::ZERO, Vec3::X, -Vec3::X, Vec3::Y] {
            let inside = assert_parity(shape, Vec3::ZERO, direction).unwrap();
            assert_eq!(inside.t, 0.0);
            assert_eq!(inside.normal, Vec3::ZERO);
            assert_eq!(assert_parity(shape, -Vec3::X, direction).unwrap().t, 0.0);
        }
        let inward = assert_parity(shape, -Vec3::X, Vec3::X * 2.0).unwrap();
        assert_eq!(inward.normal, -Vec3::X);
        assert!(assert_parity(shape, -Vec3::X * 3.0, Vec3::ZERO).is_none());
    }
}

#[test]
fn degenerate_shapes_caps_and_scaled_rotations_are_not_reinterpreted() {
    let special = [
        GltfColliderShape::Aabb(Aabb::from_center_size(Vec3::ZERO, Vec3::ZERO)),
        GltfColliderShape::Obb(Obb::new(Vec3::ZERO, Vec3::ZERO, Quat::IDENTITY)),
        GltfColliderShape::Sphere(Sphere::new(Vec3::ZERO, 0.0)),
        GltfColliderShape::Capsule(Capsule::new(Vec3::ZERO, Vec3::ZERO, 0.0)),
        GltfColliderShape::Capsule(Capsule::new(Vec3::ZERO, Vec3::ZERO, 1.0)),
        GltfColliderShape::Capsule(Capsule::new(-Vec3::Y, Vec3::Y, 1.0)),
        GltfColliderShape::Obb(Obb {
            center: Vec3::ZERO,
            half_extents: Vec3::ONE,
            rotation: Quat::from_rotation_y(0.37) * 2.0,
        }),
    ];
    for shape in special {
        for origin in [
            Vec3::ZERO,
            -Vec3::X * 3.0,
            Vec3::Y * 4.0,
            vec3(1.0, 1.0, -3.0),
        ] {
            for direction in [Vec3::ZERO, -origin, Vec3::Z * 2.0] {
                assert_parity(shape, origin, direction);
            }
        }
    }
}

#[test]
fn invalid_inputs_and_extreme_limits_preserve_delegated_behavior() {
    for shape in shapes() {
        for invalid in [Vec3::NAN, Vec3::INFINITY, vec3(0.0, f32::NEG_INFINITY, 0.0)] {
            assert!(assert_parity(shape, invalid, Vec3::X).is_none());
            assert!(assert_parity(shape, -Vec3::X * 3.0, invalid).is_none());
        }
        for (origin, direction) in [
            (-Vec3::X * f32::MAX, Vec3::X),
            (-Vec3::X * 3.0, Vec3::X * f32::MAX),
            (-Vec3::X * 3.0, Vec3::X * f32::MIN_POSITIVE),
        ] {
            assert_parity(shape, origin, direction);
        }
    }
    // Inverted AABBs retain their existing slab assumptions; the adapter must
    // not impose the stricter bounding_aabb validation before ray dispatch.
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
        for direction in [Vec3::ZERO, Vec3::X * 2.0, Vec3::Y] {
            assert_parity(shape, -Vec3::X * 3.0, direction);
        }
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
        let hit = assert_parity(shape, center - axis * 8.0, axis * 2.0).unwrap();
        assert!(hit.t > 0.0);
        assert!(hit.normal.dot(axis) < 0.0);
        assert_eq!(assert_parity(shape, center, Vec3::ZERO).unwrap().t, 0.0);
    }
}

#[test]
fn hierarchy_and_non_y_up_shapes_are_static_world_space_snapshots() {
    for up in [Vec3::Y, Vec3::Z, Vec3::X, vec3(1.0, 2.0, 3.0).normalize()] {
        let scene = scene().with_up_axis(up);
        let original = scene.clone();
        let mut pose = GltfModelPose::from_scene(&scene).hierarchy().unwrap();
        let root = pose.handle(0).unwrap();
        let rest = pose.world(root).unwrap();
        let mut snapshots = Vec::new();
        for rotation in [Quat::IDENTITY, Quat::from_rotation_z(0.37)] {
            pose.set_local_matrix(
                root,
                Mat4::from_scale_rotation_translation(
                    vec3(1.2, 0.8, 1.1),
                    rotation,
                    vec3(-4.0, 3.0, 7.0),
                ) * rest,
            )
            .unwrap();
            for collider in &scene.colliders {
                check_world_shape(collider.shape);
                let snapshot = pose.collider_shape(&scene, collider).unwrap();
                assert_ne!(snapshot, collider.shape);
                check_world_shape(snapshot);
                snapshots.push(snapshot);
            }
        }
        // Existing copies are still queried at their captured pose.
        pose.set_local_matrix(root, Mat4::from_translation(Vec3::splat(100.0)))
            .unwrap();
        for snapshot in snapshots {
            check_world_shape(snapshot);
        }
        assert_eq!(scene, original);
    }
}

#[test]
fn dispatch_allocates_nothing_and_preserves_source_shapes() {
    let shapes = shapes();
    let original = shapes;
    let mut hits = [None; 4];
    ALLOCATIONS.with(|count| count.set(Some(0)));
    for (slot, shape) in hits.iter_mut().zip(&shapes) {
        *slot = shape.ray_intersect(-Vec3::X * 4.0, Vec3::X * 2.0);
    }
    let count = ALLOCATIONS.with(|count| count.replace(None).unwrap());
    assert_eq!(count, 0);
    assert!(hits.iter().all(Option::is_some));
    assert_eq!(shapes, original);
}
