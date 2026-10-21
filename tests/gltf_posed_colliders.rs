#![cfg(feature = "gltf")]

use macroquad::prelude::*;

fn scene(shape: &str) -> GltfScene {
    GltfScene::from_bytes(
        format!(
            r#"{{"asset":{{"version":"2.0"}},"scenes":[{{"nodes":[0]}}],"nodes":[{{"extras":{{"mq_role":"collider","mq_shape":"{shape}"}}}}]}}"#,
        )
        .as_bytes(),
    )
    .unwrap()
}

#[test]
fn posed_sphere_uses_the_dominant_stretch_even_when_an_iteration_seed_misses_it() {
    let scene = scene("sphere");
    for (scale, orientation) in [1., 1e-20, 1e-5, 1e20].into_iter().flat_map(|scale| {
        [
            Quat::IDENTITY,
            Quat::from_euler(EulerRot::XYZ, 0.8, -0.3, 1.2),
        ]
        .into_iter()
        .map(move |orientation| (scale, orientation))
    }) {
        let mut pose = GltfModelPose::from_scene(&scene).hierarchy().unwrap();
        // The maximum stretch is 2 along (1,-1,0), orthogonal to (1,1,1).
        // Multiplying the latter by L^T L forever misses that eigenspace.
        let matrix = Mat4::from_quat(orientation)
            * Mat4::from_cols(
                vec4(1.5 * scale, -0.5 * scale, 0., 0.),
                vec4(-0.5 * scale, 1.5 * scale, 0., 0.),
                vec4(0., 0., scale, 0.),
                Vec4::W,
            );
        pose.set_local_matrix(pose.handle(0).unwrap(), matrix)
            .unwrap();
        let GltfColliderShape::Sphere(sphere) =
            pose.collider_shape(&scene, &scene.colliders[0]).unwrap()
        else {
            panic!("sphere collider");
        };
        assert!(
            (sphere.radius as f64 / (2. * scale as f64) - 1.).abs() < 1e-6,
            "incorrect radius {} for scale {scale}",
            sphere.radius
        );
        let point = matrix.transform_point3(vec3(1., -1., 0.).normalize());
        assert!(
            point.as_dvec3().length() <= sphere.radius as f64 * (1. + 1e-6),
            "transformed unit-sphere point {point:?} escapes {sphere:?}"
        );
        for x in -2..=2 {
            for y in -2..=2 {
                for z in -2..=2 {
                    let direction = vec3(x as f32, y as f32, z as f32);
                    if direction != Vec3::ZERO {
                        let point = matrix.transform_point3(direction.normalize());
                        assert!(
                            point.as_dvec3().length() <= sphere.radius as f64 * (1. + 1e-6),
                            "sampled transformed surface {point:?} escapes {sphere:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn posed_sphere_handles_collapsed_maps_and_finite_radius_after_huge_stretch() {
    let mut scene = scene("sphere");
    for (matrix, expected) in [
        (Mat4::from_scale(Vec3::ZERO), 0.),
        (Mat4::from_scale(vec3(0., 3., 0.)), 3.),
        (
            Mat4::from_cols(
                vec4(1., -1., 0., 0.),
                vec4(-1., 1., 0., 0.),
                Vec4::ZERO,
                Vec4::W,
            ),
            2.,
        ),
    ] {
        let mut pose = GltfModelPose::from_scene(&scene).hierarchy().unwrap();
        pose.set_local_matrix(pose.handle(0).unwrap(), matrix)
            .unwrap();
        let GltfColliderShape::Sphere(sphere) =
            pose.collider_shape(&scene, &scene.colliders[0]).unwrap()
        else {
            panic!("sphere collider");
        };
        assert_eq!(sphere.radius, expected);
    }
    // The map's stretch exceeds f32::MAX, but the final radius is finite.
    scene.colliders[0].shape = GltfColliderShape::Sphere(Sphere::new(Vec3::ZERO, 0.125));
    let matrix = Mat4::from_cols(
        vec4(3e38, -1e38, 0., 0.),
        vec4(-1e38, 3e38, 0., 0.),
        vec4(0., 0., 2e38, 0.),
        Vec4::W,
    );
    let mut pose = GltfModelPose::from_scene(&scene).hierarchy().unwrap();
    pose.set_local_matrix(pose.handle(0).unwrap(), matrix)
        .unwrap();
    let GltfColliderShape::Sphere(sphere) =
        pose.collider_shape(&scene, &scene.colliders[0]).unwrap()
    else {
        panic!("sphere collider");
    };
    assert!(sphere.radius.is_finite());
    assert!((sphere.radius as f64 / 5e37 - 1.).abs() < 1e-6);
    let before_world = pose.world(pose.handle(0).unwrap()).unwrap();
    scene.colliders[0].shape = GltfColliderShape::Sphere(Sphere::new(Vec3::ZERO, 1.));
    let before_shape = scene.colliders[0].shape;
    assert!(pose.collider_shape(&scene, &scene.colliders[0]).is_none());
    assert_eq!(pose.world(pose.handle(0).unwrap()).unwrap(), before_world);
    assert_eq!(scene.colliders[0].shape, before_shape);
}

#[test]
fn posed_capsule_radial_stretch_avoids_intermediate_overflow_and_underflow() {
    let scene = scene("capsule");
    for scale in [1e-20, 1e10, 1e20] {
        let mut pose = GltfModelPose::from_scene(&scene).hierarchy().unwrap();
        pose.set_local_matrix(
            pose.handle(0).unwrap(),
            Mat4::from_scale(vec3(scale, 1., 2. * scale)),
        )
        .unwrap();
        let GltfColliderShape::Capsule(capsule) =
            pose.collider_shape(&scene, &scene.colliders[0]).unwrap()
        else {
            panic!("capsule collider");
        };
        assert_eq!(capsule.a, -Vec3::Y);
        assert_eq!(capsule.b, Vec3::Y);
        assert!(
            (capsule.radius as f64 / (2. * scale as f64) - 1.).abs() < 1e-6,
            "incorrect radius {} for scale {scale}",
            capsule.radius
        );
    }
}
