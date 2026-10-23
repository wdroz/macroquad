use macroquad::prelude::*;

fn check_queries(triangles: Vec<Triangle>, boxes: &[Aabb], spheres: &[Sphere]) {
    let singles = triangles
        .iter()
        .map(|&triangle| TriangleMesh::new(vec![triangle]).unwrap())
        .collect::<Vec<_>>();
    let mesh = TriangleMesh::new(triangles).unwrap();
    for bounds in boxes {
        assert_eq!(
            mesh.overlaps_aabb(bounds),
            singles.iter().any(|single| single.overlaps_aabb(bounds)),
            "box {bounds:?}"
        );
    }
    for sphere in spheres {
        assert_eq!(
            mesh.overlaps_sphere(sphere),
            singles.iter().any(|single| single.overlaps_sphere(sphere)),
            "sphere {sphere:?}"
        );
    }
}

#[test]
fn mesh_bounds_pruning_matches_individual_surfaces_at_multiple_scales() {
    // Singletons bypass the complete-mesh rejection, so their union retains
    // the original per-triangle path as a reference for pruning decisions.
    for scale in [1e-25, 1.0, 1e30] {
        let triangles = (0..96)
            .map(|index| {
                let corner = vec3(
                    (index % 8) as f32 * 2.0 - 8.0,
                    (index / 8) as f32 * 2.0 - 12.0,
                    (index % 3) as f32 * 0.5,
                ) * scale;
                let b = corner + Vec3::X * scale;
                let c = if index % 11 == 0 {
                    corner // Degenerate segments are still surface geometry.
                } else {
                    corner + Vec3::Y * scale
                };
                Triangle::new(corner, b, c)
            })
            .collect::<Vec<_>>();
        let union = TriangleMesh::new(triangles.clone())
            .unwrap()
            .aabb()
            .unwrap();
        let mut boxes = Vec::new();
        let mut spheres = Vec::new();
        // Include exact surface vertices and complete-union corners/edge
        // touches, which must not turn into strict broadphase rejection.
        for p in triangles
            .iter()
            .map(|triangle| triangle.a)
            .chain([union.min, union.max])
        {
            boxes.push(Aabb::from_min_max(p, p));
            spheres.push(Sphere::new(p, 0.0));
            boxes.push(Aabb::from_min_max(p, p + Vec3::splat(scale)));
            spheres.push(Sphere::new(p + Vec3::Z * scale, scale));
        }
        let mut state = 912731_u32;
        let mut scalar = || {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            ((state >> 8) as f32 / 16777216.0) * 32.0 - 16.0
        };
        for index in 0..256 {
            let center = vec3(scalar(), scalar(), scalar()) * scale;
            let radius = if index % 7 == 0 {
                0.0
            } else {
                scalar().abs() * 0.125 * scale
            };
            boxes.push(Aabb::from_center_size(center, Vec3::splat(radius * 2.0)));
            spheres.push(Sphere::new(center, radius));
        }
        check_queries(triangles, &boxes, &spheres);
    }
}

#[test]
fn mesh_bounds_keep_extreme_finite_touches_and_invalid_query_behavior() {
    let far = f32::MAX * 0.5;
    let triangles = vec![
        Triangle::new(Vec3::X * -far, Vec3::X * far, Vec3::X * -far),
        Triangle::new(Vec3::ZERO, Vec3::Y, Vec3::Z),
        Triangle::new(Vec3::splat(far), Vec3::splat(far), Vec3::splat(far)),
    ];
    let boxes = [
        Aabb::from_min_max(Vec3::splat(-f32::MAX), Vec3::splat(f32::MAX)),
        Aabb::from_min_max(Vec3::X * far, Vec3::X * far),
        Aabb::from_min_max(Vec3::splat(far), Vec3::splat(far)),
        Aabb::from_min_max(Vec3::ONE, Vec3::ZERO),
        Aabb::from_min_max(Vec3::NAN, Vec3::ONE),
    ];
    let spheres = [
        Sphere::new(Vec3::splat(f32::MAX), f32::MAX),
        Sphere::new(Vec3::X * far, 0.0),
        Sphere::new(Vec3::splat(far), 0.0),
        Sphere::new(Vec3::Z * 0.5, 0.5),
        Sphere::new(Vec3::Z * 0.5, 0.49),
        Sphere::new(Vec3::ZERO, -1.0),
        Sphere::new(Vec3::ZERO, f32::INFINITY),
        Sphere::new(Vec3::NAN, 1.0),
    ];
    check_queries(triangles, &boxes, &spheres);
}
