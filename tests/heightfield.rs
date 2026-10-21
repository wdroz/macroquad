use macroquad::prelude::*;
fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 2e-5, "{a} != {b}");
}
fn grid(up: Vec3) -> Heightfield {
    Heightfield::with_up(
        3,
        2,
        vec![0.0, 1.0, 2.0, 2.0, 3.0, 4.0],
        vec3(2.0, 3.0, 4.0),
        vec3(10.0, -2.0, 5.0),
        up,
    )
    .unwrap()
}
#[test]
fn row_major_scale_origin_and_edges() {
    let g = grid(Vec3::Y);
    assert_eq!(g.dimensions(), (3, 2));
    assert_eq!(g.horizontal_axes(), (Vec3::X, Vec3::Z));
    assert_eq!(g.origin(), vec3(10.0, -2.0, 5.0));
    assert_eq!(g.scale(), vec3(2.0, 3.0, 4.0));
    assert_eq!(g.up(), Vec3::Y);
    assert_eq!(g.heights(), &[0.0, 1.0, 2.0, 2.0, 3.0, 4.0]);
    for (u, v, h) in [
        (0.0, 0.0, 0.0),
        (2.0, 0.0, 3.0),
        (4.0, 4.0, 12.0),
        (1.0, 2.0, 4.5),
        (3.0, 4.0, 10.5),
    ] {
        close(g.sample_height(u, v).unwrap(), h);
        let hit = g
            .ray_intersect(&Ray::new(g.origin() + vec3(u, 30.0, v), -Vec3::Y * 2.0))
            .unwrap();
        close(hit.hit.t, (30.0 - h) / 2.0);
        close(hit.hit.point.y, g.origin().y + h);
        assert!(hit.hit.normal.dot(Vec3::Y) > 0.0);
    }
    assert_eq!(
        g.aabb(),
        Aabb::from_min_max(vec3(10.0, -2.0, 5.0), vec3(14.0, 10.0, 9.0))
    );
}
#[test]
fn sampling_matches_triangles_not_bilinear() {
    let g = Heightfield::new(2, 2, vec![0.0, 0.0, 0.0, 4.0], Vec3::ONE, Vec3::ZERO).unwrap();
    for (x, z, h, index) in [
        (0.25, 0.25, 0.0, 0),
        (0.75, 0.75, 2.0, 1),
        (0.5, 0.5, 0.0, 0),
        (1.0, 1.0, 4.0, 1),
    ] {
        assert_eq!(g.sample_height(x, z), Some(h));
        let hit = g
            .ray_intersect(&Ray::new(vec3(x, 10.0, z), -Vec3::Y))
            .unwrap();
        close(hit.hit.point.y, h);
        assert_eq!(hit.index, index);
    }
}
#[test]
fn caller_up_axes_and_oblique_frames() {
    for up in [
        Vec3::X,
        Vec3::Z,
        -Vec3::Y,
        vec3(1.0, 2.0, 3.0),
        Vec3::splat(1e-30),
        Vec3::splat(f32::MAX),
    ] {
        let g = grid(up);
        let (u, v) = g.horizontal_axes();
        close(u.length(), 1.0);
        close(v.length(), 1.0);
        close(g.up().length(), 1.0);
        close(u.dot(v), 0.0);
        close(u.dot(g.up()), 0.0);
        close(v.dot(g.up()), 0.0);
        assert!(v.cross(u).dot(g.up()) > 0.9999);
        let p = g.origin() + u * 1.0 + v * 2.0 + g.up() * 4.5;
        let hit = g
            .ray_intersect(&Ray::new(p + g.up() * 10.0, -g.up() * 2.0))
            .unwrap();
        close(hit.hit.t, 5.0);
        assert!((hit.hit.point - p).length() < 2e-5);
        assert!(hit.hit.normal.dot(g.up()) > 0.0);
        assert!(g.overlaps_aabb(&Aabb::from_center_size(p, Vec3::splat(0.02))));
        assert!(!g.overlaps_aabb(&Aabb::from_center_size(
            p - g.up() * 20.0,
            Vec3::splat(0.02)
        )));
    }
}
#[test]
fn surface_overlap_and_invalid_queries() {
    let g = Heightfield::new(2, 2, vec![0.0; 4], Vec3::ONE, Vec3::ZERO).unwrap();
    for p in [Vec3::ZERO, vec3(1.0, 0.0, 1.0), vec3(0.5, 0.0, 0.5)] {
        assert!(g.overlaps_aabb(&Aabb::from_min_max(p, p)));
    }
    for p in [
        vec3(0.5, -1.0, 0.5),
        vec3(0.5, 1.0, 0.5),
        vec3(2.0, 0.0, 0.5),
        Vec3::NAN,
    ] {
        assert!(!g.overlaps_aabb(&Aabb::from_min_max(p, p)));
    }
    assert!(!g.overlaps_aabb(&Aabb::from_min_max(Vec3::ONE, Vec3::ZERO)));
    assert!(g.ray_intersect(&Ray::new(Vec3::NAN, Vec3::Y)).is_none());
    assert!(g
        .ray_intersect(&Ray::new(vec3(0.5, 1.0, 0.5), Vec3::Y))
        .is_none());
    assert!(g
        .ray_intersect(&Ray::new(vec3(0.5, -1.0, 0.5), Vec3::Y))
        .is_some());
    for (u, v) in [
        (-0.1, 0.0),
        (0.0, -0.1),
        (1.01, 0.0),
        (0.0, 1.01),
        (f32::NAN, 0.0),
        (0.0, f32::INFINITY),
    ] {
        assert!(g.sample_height(u, v).is_none());
    }
}
#[test]
fn invalid_construction_and_generated_overflow() {
    for (cols, rows, h) in [
        (0, 2, vec![]),
        (2, 1, vec![0.0; 2]),
        (2, 2, vec![0.0; 3]),
        (usize::MAX, 2, vec![]),
        (2, 2, vec![f32::NAN; 4]),
    ] {
        assert!(Heightfield::new(cols, rows, h, Vec3::ONE, Vec3::ZERO).is_none());
    }
    for scale in [
        vec3(0.0, 1.0, 1.0),
        vec3(1.0, 1.0, -1.0),
        Vec3::NAN,
        Vec3::INFINITY,
        vec3(1.0, f32::MAX, 1.0),
    ] {
        assert!(Heightfield::new(2, 2, vec![2.0; 4], scale, Vec3::ZERO).is_none());
    }
    for up in [Vec3::ZERO, Vec3::NAN, Vec3::INFINITY] {
        assert!(Heightfield::with_up(2, 2, vec![0.0; 4], Vec3::ONE, Vec3::ZERO, up).is_none());
    }
    assert!(Heightfield::new(2, 2, vec![0.0; 4], Vec3::ONE, Vec3::NAN).is_none());
    assert!(Heightfield::new(2, 2, vec![f32::MAX; 4], Vec3::ONE, Vec3::Y * f32::MAX).is_none());
    assert!(Heightfield::new(3, 2, vec![0.0; 6], vec3(f32::MAX, 1.0, 1.0), Vec3::ZERO).is_none());
}
#[test]
fn signed_height_scale_and_fractional_outer_edge() {
    for height_scale in [0.0, -2.0] {
        let g =
            Heightfield::new(4, 2, vec![2.0; 8], vec3(0.1, height_scale, 0.3), Vec3::ZERO).unwrap();
        close(g.sample_height(0.3, 0.3).unwrap(), 2.0 * height_scale);
        assert!(g
            .sample_height(f32::from_bits(0.3f32.to_bits() + 1), 0.3)
            .is_none());
        assert_eq!(g.clone().heights(), g.heights());
    }
}
#[test]
fn deterministic_samples_match_independent_cell_plane() {
    let h: Vec<_> = (0..20).map(|i| ((i * 17 % 13) as f32) - 6.0).collect();
    let g = Heightfield::new(5, 4, h.clone(), vec3(2.0, 0.5, 3.0), Vec3::ZERO).unwrap();
    let mut state = 8193u32;
    for _ in 0..4000 {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        let x = (state >> 8) as f32 / 16777216.0 * 4.0;
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        let z = (state >> 8) as f32 / 16777216.0 * 3.0;
        let (c, r) = (x.floor() as usize, z.floor() as usize);
        let (a, b) = (x - c as f32, z - r as f32);
        let i = r * 5 + c;
        let expected = if a + b <= 1.0 {
            h[i] + (h[i + 1] - h[i]) * a + (h[i + 5] - h[i]) * b
        } else {
            h[i + 6] + (h[i + 5] - h[i + 6]) * (1.0 - a) + (h[i + 1] - h[i + 6]) * (1.0 - b)
        };
        close(g.sample_height(x * 2.0, z * 3.0).unwrap(), expected * 0.5);
        let ray = Ray::new(vec3(x * 2.0, 20.0, z * 3.0), -Vec3::Y);
        let hit = g.ray_intersect(&ray).unwrap();
        close(hit.hit.point.y, expected * 0.5);
        assert!(g.overlaps_aabb(&Aabb::from_center_size(hit.hit.point, Vec3::splat(0.001))));
    }
}

#[test]
fn rounded_down_edge_uses_final_sample_exactly() {
    let g = Heightfield::new(
        8,
        2,
        (0..16).map(|i| i as f32).collect(),
        vec3(0.1, 1.0, 0.1),
        Vec3::ZERO,
    )
    .unwrap();
    assert_eq!(g.sample_height(0.7, 0.1), Some(15.0));
}
