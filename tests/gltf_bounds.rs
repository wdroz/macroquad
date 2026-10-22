#![cfg(feature = "gltf")]

use macroquad::prelude::*;
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

fn bits(bounds: Option<Aabb>) -> Option<[u32; 6]> {
    bounds.map(|bounds| {
        [
            bounds.min.x.to_bits(),
            bounds.min.y.to_bits(),
            bounds.min.z.to_bits(),
            bounds.max.x.to_bits(),
            bounds.max.y.to_bits(),
            bounds.max.z.to_bits(),
        ]
    })
}

#[test]
fn aabb_dispatch_preserves_every_valid_corner_bit_and_rejects_invalid_bounds() {
    for bounds in [
        Aabb::from_min_max(vec3(-0., 0., -0.), vec3(0., -0., 0.)),
        Aabb::from_min_max(Vec3::splat(-f32::MAX), Vec3::splat(f32::MAX)),
        Aabb::from_min_max(vec3(-3., 1., -2.), vec3(-3., 7., 5.)),
    ] {
        assert_eq!(
            bits(GltfColliderShape::Aabb(bounds).bounding_aabb()),
            bits(Some(bounds))
        );
    }
    for bounds in [
        Aabb::from_min_max(Vec3::ONE, Vec3::ZERO),
        Aabb::from_min_max(Vec3::NAN, Vec3::ONE),
        Aabb::from_min_max(-Vec3::ONE, Vec3::splat(f32::INFINITY)),
    ] {
        assert!(GltfColliderShape::Aabb(bounds).bounding_aabb().is_none());
    }
}

fn assert_primitive_parity(shape: GltfColliderShape) {
    let expected = match shape {
        GltfColliderShape::Aabb(bounds) => Some(bounds),
        GltfColliderShape::Sphere(shape) => shape.bounding_aabb(),
        GltfColliderShape::Obb(shape) => shape.bounding_aabb(),
        GltfColliderShape::Capsule(shape) => shape.bounding_aabb(),
    };
    assert_eq!(bits(shape.bounding_aabb()), bits(expected), "{shape:?}");
}

#[test]
fn non_aabb_variants_delegate_success_degeneracy_and_failure_unchanged() {
    for radius in [0., 0.3, -1., f32::NAN, f32::INFINITY, f32::MAX] {
        for center in [
            Vec3::ZERO,
            vec3(-3., 4., 7.),
            Vec3::splat(f32::MAX),
            Vec3::NAN,
        ] {
            assert_primitive_parity(GltfColliderShape::Sphere(Sphere::new(center, radius)));
            for end in [center, Vec3::Y, Vec3::splat(f32::MAX), Vec3::NAN] {
                assert_primitive_parity(GltfColliderShape::Capsule(Capsule::new(
                    center, end, radius,
                )));
            }
        }
    }
    for extents in [
        Vec3::ZERO,
        vec3(1., 0., 2.),
        -Vec3::ONE,
        Vec3::NAN,
        Vec3::splat(f32::MAX),
    ] {
        for rotation in [
            Quat::IDENTITY,
            Quat::from_euler(EulerRot::XYZ, 0.3, 0.7, -0.9) * 2.,
            Quat::from_xyzw(0., 0., 0., 0.),
            Quat::from_xyzw(0., f32::NAN, 0., 1.),
        ] {
            assert_primitive_parity(GltfColliderShape::Obb(Obb::new(
                Vec3::ONE,
                extents,
                rotation,
            )));
        }
    }
}

fn scene() -> GltfScene {
    GltfScene::from_bytes(
        br#"{
      "asset":{"version":"2.0"}, "scenes":[{"nodes":[0]}],
      "nodes":[
        {"name":"root", "translation":[1,2,3], "children":[1,2,3,4]},
        {"name":"box_collider", "translation":[-6,0,0], "extras":{"mq_shape":"box"}},
        {"name":"turned_collider", "translation":[-2,0,0],
         "rotation":[0,0.38268343,0,0.92387953], "scale":[2,1,0.5],
         "extras":{"mq_shape":"box"}},
        {"name":"orb_collider", "translation":[2,0,0], "extras":{"mq_shape":"sphere"}},
        {"name":"post_trigger", "translation":[6,0,0], "scale":[0.5,2,0.5],
         "extras":{"mq_shape":"capsule","mq_tag":"goal"}}
      ]
    }"#,
    )
    .unwrap()
}

// The caller owns this mapping; grid indices need not equal node indices.
fn caller_index(index: usize) -> u32 {
    index as u32 * 7 + 3
}

fn check_queries(shapes: &[GltfColliderShape], grid: &UniformGrid) {
    for x in -4..=4 {
        for y in -3..=3 {
            for z in -3..=3 {
                let center = vec3(x as f32 * 2., y as f32 * 2., z as f32 * 2.);
                let aabb = Aabb::from_center_size(center, vec3(0.7, 1.3, 2.1));
                let complete: Vec<_> = shapes
                    .iter()
                    .enumerate()
                    .filter(|(_, shape)| shape.overlaps_aabb(&aabb))
                    .map(|(index, _)| caller_index(index))
                    .collect();
                let candidates = grid.query_aabb(&aabb);
                let narrowed: Vec<_> = candidates
                    .into_iter()
                    .filter(|&index| shapes[((index - 3) / 7) as usize].overlaps_aabb(&aabb))
                    .collect();
                assert_eq!(narrowed, complete, "AABB {aabb:?}");

                let sphere = Sphere::new(center, 0.6);
                let complete: Vec<_> = shapes
                    .iter()
                    .enumerate()
                    .filter(|(_, shape)| shape.overlaps_sphere(&sphere))
                    .map(|(index, _)| caller_index(index))
                    .collect();
                let narrowed: Vec<_> = grid
                    .query_aabb(&sphere.bounding_aabb().unwrap())
                    .into_iter()
                    .filter(|&index| shapes[((index - 3) / 7) as usize].overlaps_sphere(&sphere))
                    .collect();
                assert_eq!(narrowed, complete, "sphere {sphere:?}");
            }
        }
    }
    for shape in shapes {
        let center = shape.bounding_aabb().unwrap().center();
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            let body = Sphere::new(center - axis * 10., 0.4);
            let delta = axis * 20.;
            let start = body.bounding_aabb().unwrap();
            let end = body.offset(delta).bounding_aabb().unwrap();
            // This is one straight translation, not a slide/step envelope.
            let envelope = Aabb::from_min_max(start.min.min(end.min), start.max.max(end.max));
            let complete: Vec<_> = shapes
                .iter()
                .enumerate()
                .filter_map(|(index, shape)| {
                    shape
                        .sweep_sphere(&body, delta)
                        .map(|hit| (caller_index(index), hit))
                })
                .collect();
            let narrowed: Vec<_> = grid
                .query_aabb(&envelope)
                .into_iter()
                .filter_map(|index| {
                    shapes[((index - 3) / 7) as usize]
                        .sweep_sphere(&body, delta)
                        .map(|hit| (index, hit))
                })
                .collect();
            assert_eq!(narrowed, complete);
        }
    }
}

#[test]
fn posed_and_non_y_up_snapshots_rebuild_to_complete_narrowphase_results() {
    for up in [Vec3::Y, Vec3::Z, Vec3::X, vec3(1., 2., 3.).normalize()] {
        let scene = scene().with_up_axis(up);
        let original = scene.clone();
        let mut grid = UniformGrid::new(2.).unwrap();
        let shapes: Vec<_> = scene
            .colliders
            .iter()
            .map(|collider| collider.shape)
            .collect();
        let saved: Vec<_> = shapes
            .iter()
            .map(GltfColliderShape::bounding_aabb)
            .collect();
        for (index, shape) in shapes.iter().enumerate() {
            assert_primitive_parity(*shape);
            assert!(grid.insert(caller_index(index), shape.bounding_aabb().unwrap()));
        }
        check_queries(&shapes, &grid);
        let mut pose = GltfModelPose::from_scene(&scene).hierarchy().unwrap();
        let root = pose.handle(0).unwrap();
        let replacement = Mat4::from_scale_rotation_translation(
            vec3(1.2, 0.8, 1.1),
            Quat::from_euler(EulerRot::XYZ, 0.3, -0.4, 0.2),
            vec3(-3., 1., 2.),
        ) * pose.world(root).unwrap();
        pose.set_local_matrix(root, replacement).unwrap();
        let posed: Vec<_> = scene
            .colliders
            .iter()
            .map(|collider| pose.collider_shape(&scene, collider).unwrap())
            .collect();
        for shape in &posed {
            assert_primitive_parity(*shape);
        }
        assert_eq!(
            grid.rebuild(
                posed
                    .iter()
                    .enumerate()
                    .map(|(index, shape)| (caller_index(index), shape.bounding_aabb().unwrap()))
            ),
            shapes.len()
        );
        check_queries(&posed, &grid);
        // Previously returned shapes/bounds do not follow the editable pose.
        assert_eq!(
            shapes
                .iter()
                .map(GltfColliderShape::bounding_aabb)
                .collect::<Vec<_>>(),
            saved
        );
        assert_ne!(posed, shapes);
        assert_eq!(scene, original);
    }
}

#[test]
fn bounds_candidates_still_need_the_exact_shape_query() {
    let shape = GltfColliderShape::Sphere(Sphere::new(Vec3::ZERO, 1.));
    let mut grid = UniformGrid::new(2.).unwrap();
    assert!(grid.insert(42, shape.bounding_aabb().unwrap()));
    let corner = Aabb::from_min_max(Vec3::splat(0.9), Vec3::splat(0.9));
    assert_eq!(grid.query_aabb(&corner), [42]);
    assert!(!shape.overlaps_aabb(&corner));
}

#[test]
fn dispatch_allocates_nothing_and_leaves_source_shapes_unchanged() {
    let scene = scene();
    let shapes: Vec<_> = scene
        .colliders
        .iter()
        .map(|collider| collider.shape)
        .collect();
    let original = shapes.clone();
    let mut result = [None; 4];
    ALLOCATIONS.with(|count| count.set(Some(0)));
    for (slot, shape) in result.iter_mut().zip(&shapes) {
        *slot = shape.bounding_aabb();
    }
    let count = ALLOCATIONS.with(|count| count.replace(None).unwrap());
    assert_eq!(count, 0);
    assert!(result.iter().all(Option::is_some));
    assert_eq!(shapes, original);
}
