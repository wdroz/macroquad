use macroquad::prelude::*;

fn slope() -> Triangle {
    Triangle::new(Vec3::ZERO, vec3(2.0, 0.0, 2.0), vec3(0.0, 2.0, 2.0))
}
fn mesh(t: Triangle) -> TriangleMesh {
    TriangleMesh::new(vec![t]).unwrap()
}
fn point(p: Vec3) -> Aabb {
    Aabb::from_min_max(p, p)
}

#[test]
fn nearest_hit_retains_original_indices_and_ray_units() {
    let t = slope();
    let far = Triangle::new(
        t.a - Vec3::Z * 3.0,
        t.b - Vec3::Z * 3.0,
        t.c - Vec3::Z * 3.0,
    );
    let m = TriangleMesh::new(vec![far, t, t]).unwrap();
    let ray = Ray::new(vec3(0.5, 0.5, 5.0), -Vec3::Z * 2.0);
    let hit = m.ray_intersect(&ray).unwrap();
    assert_eq!(hit.index, 1);
    assert_eq!(hit.hit.t, 2.0);
    assert_eq!(hit.hit.point, vec3(0.5, 0.5, 1.0));
    assert!((hit.hit.normal - vec3(-1.0, -1.0, 1.0).normalize()).length() < 1e-6);
    assert_eq!(m.ray_intersect(&ray), ray.closest_triangle(m.triangles()));
    assert_eq!(
        m.ray_intersect(&Ray::new(hit.hit.point, Vec3::ZERO))
            .unwrap()
            .hit
            .t,
        0.0
    );
    assert!(m
        .ray_intersect(&Ray::new(Vec3::splat(10.0), Vec3::Z))
        .is_none());
    assert!(m.ray_intersect(&Ray::new(Vec3::NAN, Vec3::Z)).is_none());
    assert!(m
        .ray_intersect(&Ray::new(Vec3::ZERO, Vec3::INFINITY))
        .is_none());
}

#[test]
fn bounds_are_tight_and_clone_cannot_stale_original() {
    let mut input = vec![slope()];
    let m = TriangleMesh::new(input.clone()).unwrap();
    input[0].a = Vec3::splat(100.0);
    assert_eq!(
        m.aabb(),
        Some(Aabb::from_min_max(Vec3::ZERO, Vec3::splat(2.0)))
    );
    assert_eq!(m.triangle_bounds(), &[slope().aabb()]);
    assert_eq!(m.clone().triangles(), &[slope()]);
    assert_eq!(m.triangles(), &[slope()]);
}

#[test]
fn sloped_box_query_rejects_bounds_only_false_positives() {
    let m = mesh(slope());
    assert!(m.overlaps_aabb(&point(vec3(0.5, 0.5, 1.0))));
    assert!(m.overlaps_aabb(&Aabb::from_center_size(
        vec3(0.5, 0.5, 1.0),
        Vec3::splat(0.1)
    )));
    assert!(!m.overlaps_aabb(&Aabb::from_center_size(
        vec3(0.5, 0.5, 1.8),
        Vec3::splat(0.1)
    )));
    // Same plane and inside triangle bounds, but outside its hypotenuse.
    let xy = mesh(Triangle::new(Vec3::ZERO, Vec3::X * 2.0, Vec3::Y * 2.0));
    assert!(!xy.overlaps_aabb(&Aabb::from_center_size(
        vec3(1.8, 1.8, 0.0),
        Vec3::splat(0.1)
    )));
    assert!(m.overlaps_aabb(&point(Vec3::ZERO)));
    assert!(m.overlaps_aabb(&point(Vec3::X + Vec3::Z)));
}

#[test]
fn sphere_face_edge_vertex_and_surface_semantics() {
    let m = mesh(Triangle::new(Vec3::ZERO, Vec3::X * 2.0, Vec3::Y * 2.0));
    assert!(m.overlaps_sphere(&Sphere::new(vec3(0.5, 0.5, 1.0), 1.0)));
    assert!(!m.overlaps_sphere(&Sphere::new(vec3(0.5, 0.5, 1.01), 1.0)));
    assert!(m.overlaps_sphere(&Sphere::new(vec3(1.0, -1.0, 0.0), 1.0)));
    assert!(m.overlaps_sphere(&Sphere::new(vec3(-1.0, 0.0, 0.0), 1.0)));
    assert!(!m.overlaps_sphere(&Sphere::new(vec3(1.8, 1.8, 0.0), 0.1)));
    assert!(m.overlaps_sphere(&Sphere::new(Vec3::ZERO, 0.0)));
    assert!(mesh(slope()).overlaps_sphere(&Sphere::new(vec3(0.5, 0.5, 1.0), 0.0)));
    let distant = mesh(Triangle::new(
        Vec3::X * 5.0,
        vec3(5.0, 2.0, 0.0),
        vec3(5.0, 0.0, 2.0),
    ));
    assert!(!distant.overlaps_sphere(&Sphere::new(Vec3::ZERO, 1.0)));
}

#[test]
fn invalid_inputs_and_empty_meshes_miss() {
    for m in [TriangleMesh::default(), TriangleMesh::new(vec![]).unwrap()] {
        assert_eq!(m.aabb(), None);
        assert!(m.triangles().is_empty());
        assert!(m.triangle_bounds().is_empty());
        assert!(!m.overlaps_aabb(&point(Vec3::ZERO)));
        assert!(!m.overlaps_sphere(&Sphere::new(Vec3::ZERO, 100.0)));
        assert!(m.ray_intersect(&Ray::default()).is_none());
    }
    assert!(TriangleMesh::new(vec![Triangle::new(Vec3::ZERO, Vec3::NAN, Vec3::X)]).is_none());
    let m = mesh(slope());
    for b in [
        Aabb::from_min_max(Vec3::ONE, Vec3::ZERO),
        point(Vec3::NAN),
        point(Vec3::INFINITY),
    ] {
        assert!(!m.overlaps_aabb(&b));
    }
    for s in [
        Sphere::new(Vec3::ZERO, -1.0),
        Sphere::new(Vec3::ZERO, f32::NAN),
        Sphere::new(Vec3::ZERO, f32::INFINITY),
        Sphere::new(Vec3::NAN, 1.0),
    ] {
        assert!(!m.overlaps_sphere(&s));
    }
}

#[test]
fn degenerate_faces_overlap_as_segments_and_points_but_rays_miss() {
    for t in [
        Triangle::new(Vec3::ZERO, Vec3::X * 2.0, Vec3::X),
        Triangle::new(Vec3::ZERO, Vec3::ZERO, Vec3::ZERO),
    ] {
        let m = mesh(t);
        assert!(m.overlaps_aabb(&point(Vec3::ZERO)));
        assert!(m.overlaps_sphere(&Sphere::new(-Vec3::X, 1.0)));
        assert!(!m.overlaps_aabb(&point(Vec3::Y)));
        assert!(!m.overlaps_sphere(&Sphere::new(Vec3::Y * 2.0, 1.0)));
        assert!(m.ray_intersect(&Ray::new(Vec3::ZERO, Vec3::Z)).is_none());
    }
    let t = Triangle::new(Vec3::ZERO, Vec3::splat(2.0), Vec3::ONE);
    assert!(!mesh(t).overlaps_aabb(&Aabb::from_center_size(
        vec3(1.0, 0.0, 1.0),
        Vec3::splat(0.1)
    )));
}

#[test]
fn overlap_wide_intermediates_handle_extreme_finite_coordinates() {
    let large = f32::MAX * 0.5;
    for scale in [large, 1e-25] {
        let m = mesh(Triangle::new(Vec3::ZERO, Vec3::X * scale, Vec3::Y * scale));
        let p = vec3(scale * 0.25, scale * 0.25, 0.0);
        assert!(m.overlaps_aabb(&point(p)));
        assert!(m.overlaps_sphere(&Sphere::new(p, 0.0)));
        assert!(!m.overlaps_sphere(&Sphere::new(p + Vec3::Z * scale, scale * 0.5)));
    }
}

#[test]
fn axis_permutations_winding_and_scale_preserve_overlap_results() {
    let permute = |v: Vec3, axis| match axis {
        0 => v,
        1 => vec3(v.z, v.x, v.y),
        _ => vec3(v.y, v.z, v.x),
    };
    for axis in 0..3 {
        for scale in [0.001, 1.0, 1000.0] {
            for reversed in [false, true] {
                let t = slope();
                let [a, b, c] = [t.a, t.b, t.c].map(|v| permute(v * scale, axis));
                let m = mesh(if reversed {
                    Triangle::new(c, b, a)
                } else {
                    Triangle::new(a, b, c)
                });
                let center = permute(vec3(0.5, 0.5, 1.0) * scale, axis);
                assert!(m.overlaps_aabb(&Aabb::from_center_size(center, Vec3::splat(scale * 0.1))));
                assert!(m.overlaps_sphere(&Sphere::new(center, scale * 0.1)));
                let miss = permute(vec3(1.9, 1.9, 0.1) * scale, axis);
                assert!(!m.overlaps_aabb(&Aabb::from_center_size(miss, Vec3::splat(scale * 0.1))));
                assert!(!m.overlaps_sphere(&Sphere::new(miss, scale * 0.1)));
            }
        }
    }
}

// Independent oracle: clip the triangle polygon against six box planes.
fn clipped_overlap(t: Triangle, b: Aabb) -> bool {
    let mut polygon = vec![t.a.as_dvec3(), t.b.as_dvec3(), t.c.as_dvec3()];
    for axis in 0..3 {
        for (boundary, sign) in [(b.min[axis] as f64, 1.0), (b.max[axis] as f64, -1.0)] {
            if polygon.is_empty() {
                return false;
            }
            let old = std::mem::take(&mut polygon);
            let mut prev = *old.last().unwrap();
            let mut dp = (prev[axis] - boundary) * sign;
            for cur in old {
                let dc = (cur[axis] - boundary) * sign;
                if (dp >= 0.0) != (dc >= 0.0) {
                    polygon.push(prev + (cur - prev) * (dp / (dp - dc)));
                }
                if dc >= 0.0 {
                    polygon.push(cur);
                }
                prev = cur;
                dp = dc;
            }
        }
    }
    !polygon.is_empty()
}

#[test]
fn box_sat_matches_independent_polygon_clipping() {
    let mut state = 814723u32;
    let mut scalar = || {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        ((state >> 8) as f32 / 16777216.0) * 8.0 - 4.0
    };
    for i in 0..12000 {
        let t = Triangle::new(
            vec3(scalar(), scalar(), scalar()),
            vec3(scalar(), scalar(), scalar()),
            vec3(scalar(), scalar(), scalar()),
        );
        let center = vec3(scalar(), scalar(), scalar());
        let size = vec3(scalar().abs(), scalar().abs(), scalar().abs());
        let b = Aabb::from_center_size(center, size);
        assert_eq!(
            mesh(t).overlaps_aabb(&b),
            clipped_overlap(t, b),
            "case {i}: {t:?} {b:?}"
        );
    }
}

#[test]
fn enclosed_volume_does_not_imply_surface_overlap() {
    let vertices = [Vec3::ZERO, Vec3::X * 4.0, Vec3::Y * 4.0, Vec3::Z * 4.0];
    let [a, b, c, d] = vertices;
    let m = TriangleMesh::new(vec![
        Triangle::new(a, b, c),
        Triangle::new(a, b, d),
        Triangle::new(a, c, d),
        Triangle::new(b, c, d),
    ])
    .unwrap();
    assert!(!m.overlaps_sphere(&Sphere::new(Vec3::splat(0.5), 0.1)));
    assert!(!m.overlaps_aabb(&Aabb::from_center_size(Vec3::splat(0.5), Vec3::splat(0.1))));
    assert!(m.overlaps_sphere(&Sphere::new(Vec3::splat(0.5), 0.5)));
}
