//! Public-API regressions for the full translating box/static capsule pair.

use macroquad::math::{vec3, Aabb, Capsule, DVec3, Quat, Sphere, SweepHit, Vec3};

fn close(actual: Vec3, expected: Vec3, tolerance: f32) {
    assert!(
        (actual - expected).length() <= tolerance,
        "{actual:?} != {expected:?}"
    );
}

fn shared_contact(body: Aabb, delta: Vec3, capsule: Capsule, hit: SweepHit) {
    assert!((0.0..=1.0).contains(&hit.t));
    assert!((hit.normal.length() - 1.0).abs() < 2e-5, "{hit:?}");
    assert!(
        body.offset(delta * hit.t)
            .expanded(3e-4)
            .contains(hit.point),
        "{hit:?}"
    );
    let axis = capsule.b - capsule.a;
    let s = ((hit.point - capsule.a).dot(axis) / axis.length_squared()).clamp(0.0, 1.0);
    let radial = hit.point - (capsule.a + axis * s);
    assert!(
        (radial.length() - capsule.radius).abs() < 3e-4,
        "{hit:?}, {capsule:?}"
    );
    if capsule.radius > 0.0 {
        close(hit.normal, radial.normalize(), 4e-4);
    }
    assert!(delta.dot(hit.normal) < 0.0);
}

#[test]
fn thin_body_crosses_cylinder_with_both_endpoints_clear() {
    let capsule = Capsule::new(vec3(0.0, -4.0, 0.0), vec3(0.0, 4.0, 0.0), 0.2);
    let body = Aabb::from_center_size(vec3(-3.0, 0.0, 0.0), vec3(0.02, 2.0, 0.5));
    let delta = Vec3::X * 6.0;
    assert!(!capsule.overlaps_aabb(&body));
    assert!(!capsule.overlaps_aabb(&body.offset(delta)));
    let hit = body.sweep_capsule(delta, &capsule).unwrap();
    assert!((hit.t - 2.79 / 6.0).abs() < 1e-6, "{hit:?}");
    close(hit.normal, -Vec3::X, 1e-6);
    shared_contact(body, delta, capsule, hit);
}

#[test]
fn both_end_caps_hit_and_axis_direction_does_not_change_the_answer() {
    let capsule = Capsule::new(vec3(3.0, -1.0, 2.0), vec3(3.0, 1.0, 2.0), 0.5);
    for sign in [-1.0, 1.0] {
        let body = Aabb::from_center_size(vec3(3.0, 4.0 * sign, 2.0), Vec3::splat(0.2));
        let delta = Vec3::Y * (-8.0 * sign);
        let hit = body.sweep_capsule(delta, &capsule).unwrap();
        assert!((hit.t - 2.4 / 8.0).abs() < 1e-6);
        close(hit.normal, Vec3::Y * sign, 1e-6);
        shared_contact(body, delta, capsule, hit);
        let reversed = Capsule::new(capsule.b, capsule.a, capsule.radius);
        let other = body.sweep_capsule(delta, &reversed).unwrap();
        assert!((other.t - hit.t).abs() < 1e-6);
        close(other.normal, hit.normal, 1e-6);
        close(other.point, hit.point, 1e-6);
    }
}

#[test]
fn tall_body_hits_a_capsule_above_its_center_path() {
    let capsule = Capsule::new(vec3(0.0, 1.1, -0.2), vec3(0.0, 1.4, 0.2), 0.15);
    let body = Aabb::from_center_size(vec3(-3.0, 0.0, 0.0), vec3(0.05, 3.0, 0.5));
    let delta = Vec3::X * 6.0;
    assert!(capsule.ray_intersect(body.center(), delta).is_none());
    assert!(Aabb::from_center_size(body.center(), Vec3::splat(0.05))
        .sweep_capsule(delta, &capsule)
        .is_none());
    let hit = body.sweep_capsule(delta, &capsule).unwrap();
    assert!((hit.t - 2.825 / 6.0).abs() < 1e-6);
    shared_contact(body, delta, capsule, hit);
}

#[test]
fn rotated_cylinder_edge_pair_and_corner_graze_have_world_contacts() {
    let axis = vec3(0.0, 1.0, 1.0).normalize();
    let origin = vec3(12.0, -7.0, 4.0);
    let capsule = Capsule::new(origin - axis * 2.0, origin + axis * 2.0, 0.5);
    let body = Aabb::from_min_max(
        origin + vec3(-3.0, 0.2, -0.8),
        origin + vec3(-2.9, 1.0, -0.2),
    );
    let delta = vec3(6.0, 0.1, -0.05);
    let hit = body.sweep_capsule(delta, &capsule).unwrap();
    assert!(
        hit.normal.x < -0.5 && hit.normal.y > 0.0 && hit.normal.z < 0.0,
        "{hit:?}"
    );
    assert!((hit.normal.dot(axis)).abs() < 2e-5);
    shared_contact(body, delta, capsule, hit);
    assert!(!capsule.overlaps_aabb(&body.offset(delta * (hit.t - 0.001))));
    assert!(capsule.overlaps_aabb(&body.offset(delta * (hit.t + 0.001))));

    let capsule = Capsule::new(vec3(0.0, -2.0, 0.0), vec3(0.0, 2.0, 0.0), 1.0);
    let body = Aabb::from_min_max(vec3(-3.0, 2.4, 0.3), vec3(-2.9, 3.0, 0.8));
    let hit = body.sweep_capsule(Vec3::X * 6.0, &capsule).unwrap();
    let point = vec3(-0.75_f32.sqrt(), 2.4, 0.3);
    close(hit.point, point, 2e-6);
    close(hit.normal, point - capsule.b, 2e-6);
    shared_contact(body, Vec3::X * 6.0, capsule, hit);
}

#[test]
fn cylinder_graze_does_not_use_the_capsules_bounding_box() {
    let capsule = Capsule::new(vec3(0.0, -2.0, 0.0), vec3(0.0, 2.0, 0.0), 1.0);
    let body = Aabb::from_min_max(vec3(-3.0, -0.2, 0.6), vec3(-2.9, 0.2, 1.2));
    let hit = body.sweep_capsule(Vec3::X * 6.0, &capsule).unwrap();
    assert!((hit.t - 2.1 / 6.0).abs() < 1e-6);
    close(hit.normal, vec3(-0.8, 0.0, 0.6), 2e-6);
    shared_contact(body, Vec3::X * 6.0, capsule, hit);
    let miss = Aabb::from_min_max(vec3(-3.0, 2.8, 0.8), vec3(-2.9, 3.0, 1.0));
    assert!(miss.sweep_capsule(Vec3::X * 6.0, &capsule).is_none());
}

#[test]
fn every_world_axis_and_motion_sign_can_hit_a_cylinder_side() {
    for i in 0..3 {
        for j in 0..3 {
            if i == j {
                continue;
            }
            let mut spine = Vec3::ZERO;
            spine[i] = 2.0;
            let capsule = Capsule::new(-spine, spine, 0.5);
            for sign in [-1.0, 1.0] {
                let mut direction = Vec3::ZERO;
                direction[j] = sign;
                let body = Aabb::from_center_size(-direction * 3.0, Vec3::ONE);
                let delta = direction * 6.0;
                let hit = body.sweep_capsule(delta, &capsule).unwrap();
                assert!((hit.t - 1.0 / 3.0).abs() < 1e-6);
                close(hit.normal, -direction, 1e-6);
                shared_contact(body, delta, capsule, hit);
            }
        }
    }
}

#[test]
fn initial_overlap_and_containment_preserve_the_existing_contact_point() {
    let capsule = Capsule::new(vec3(0.0, -2.0, 0.0), vec3(0.0, 2.0, 0.0), 1.0);
    for body in [
        Aabb::from_center_size(vec3(0.8, 0.0, 0.0), Vec3::ONE),
        Aabb::from_center_size(Vec3::ZERO, Vec3::splat(0.2)),
        Aabb::from_center_size(Vec3::ZERO, Vec3::splat(8.0)),
    ] {
        let contact = capsule.contact_aabb(&body).unwrap();
        assert!(contact.penetration > 1e-4);
        for delta in [Vec3::ZERO, vec3(7.0, -4.0, 3.0)] {
            let hit = body.sweep_capsule(delta, &capsule).unwrap();
            assert_eq!(hit.t, 0.0);
            assert_eq!(hit.normal, -contact.normal);
            assert_eq!(hit.point, contact.point);
        }
    }
}

#[test]
fn resting_and_shallow_touches_only_block_inward_motion() {
    let capsule = Capsule::new(vec3(0.0, -2.0, 0.0), vec3(0.0, 2.0, 0.0), 1.0);
    for overlap in [0.0, 5e-5] {
        let body = Aabb::from_min_max(vec3(-2.0, -0.5, -0.5), vec3(-1.0 + overlap, 0.5, 0.5));
        for delta in [Vec3::ZERO, Vec3::Y, -Vec3::X] {
            assert!(body.sweep_capsule(delta, &capsule).is_none());
        }
        let hit = body.sweep_capsule(Vec3::X, &capsule).unwrap();
        assert_eq!(hit.t, 0.0);
        assert_eq!(hit.normal, -Vec3::X);
    }
}

#[test]
fn exact_future_tangencies_miss_but_a_nearby_inward_graze_hits() {
    let capsule = Capsule::new(-Vec3::Y * 2.0, Vec3::Y * 2.0, 1.0);
    for y in [0.0, 2.0] {
        let point = vec3(-3.0, y, 1.0);
        let body = Aabb::from_min_max(point, point);
        assert!(body.sweep_capsule(Vec3::X * 6.0, &capsule).is_none());
        let point = vec3(-3.0, y, 0.9999);
        let body = Aabb::from_min_max(point, point);
        shared_contact(
            body,
            Vec3::X * 6.0,
            capsule,
            body.sweep_capsule(Vec3::X * 6.0, &capsule).unwrap(),
        );
    }
    let point = vec3(-3.0, 3.0, 0.0);
    assert!(Aabb::from_min_max(point, point)
        .sweep_capsule(Vec3::X * 6.0, &capsule)
        .is_none());
}

#[test]
fn zero_radius_segment_embedded_in_a_flat_box_uses_initial_contact() {
    let capsule = Capsule::new(-Vec3::Y * 2.0, Vec3::Y * 2.0, 0.0);
    let body = Aabb::from_min_max(vec3(-0.5, 0.0, -0.5), vec3(0.5, 0.0, 0.5));
    let contact = capsule.contact_aabb(&body).unwrap();
    assert!(contact.penetration > 1e-4);
    for delta in [Vec3::ZERO, -contact.normal * 4.0] {
        let hit = body.sweep_capsule(delta, &capsule).unwrap();
        assert_eq!(hit.t, 0.0);
        assert_eq!(hit.normal, -contact.normal);
        assert_eq!(hit.point, contact.point);
    }
}

#[test]
fn zero_segment_length_exactly_reuses_the_sphere_query() {
    let center = vec3(4.0, -3.0, 2.0);
    let capsule = Capsule::new(center, center, 0.8);
    let sphere = Sphere::new(center, 0.8);
    for body in [
        Aabb::from_center_size(center - Vec3::X * 3.0, Vec3::ONE),
        Aabb::from_center_size(center, Vec3::ONE),
        Aabb::from_center_size(center + vec3(0.0, 0.9, 0.9), Vec3::splat(0.1)),
    ] {
        for delta in [Vec3::ZERO, Vec3::X * 6.0, vec3(6.0, -0.2, 0.1)] {
            assert_eq!(
                body.sweep_capsule(delta, &capsule),
                body.sweep_sphere(delta, &sphere)
            );
        }
    }
}

#[test]
fn zero_radius_segments_and_flat_point_boxes_can_cross() {
    let capsule = Capsule::new(-Vec3::Y, Vec3::Y, 0.0);
    for size in [Vec3::ZERO, vec3(0.0, 0.8, 0.0), Vec3::ONE] {
        let body = Aabb::from_center_size(-Vec3::X * 3.0, size);
        let hit = body.sweep_capsule(Vec3::X * 6.0, &capsule).unwrap();
        assert!((hit.t - (3.0 - size.x * 0.5) / 6.0).abs() < 1e-6, "{hit:?}");
        close(hit.normal, -Vec3::X, 1e-6);
        shared_contact(body, Vec3::X * 6.0, capsule, hit);
    }
    // The point moves along the axis, first meeting the finite segment's end.
    let body = Aabb::from_min_max(-Vec3::Y * 3.0, -Vec3::Y * 3.0);
    let hit = body.sweep_capsule(Vec3::Y * 6.0, &capsule).unwrap();
    assert!((hit.t - 1.0 / 3.0).abs() < 1e-6);
    close(hit.normal, -Vec3::Y, 1e-6);

    let oblique = Capsule::new(vec3(-1.0, -1.0, 0.0), vec3(1.0, 1.0, 0.0), 0.0);
    let body = Aabb::from_min_max(vec3(-2.0, 0.2, 0.0), vec3(-2.0, 0.2, 0.0));
    let hit = body.sweep_capsule(Vec3::X * 4.0, &oblique).unwrap();
    assert!((hit.t - 0.55).abs() < 1e-6, "{hit:?}");
    close(hit.normal, vec3(-1.0, 1.0, 0.0).normalize(), 1e-6);
}

#[test]
fn invalid_inputs_and_short_or_outward_moves_miss() {
    let capsule = Capsule::new(-Vec3::Y, Vec3::Y, 0.5);
    let body = Aabb::from_center_size(-Vec3::X * 3.0, Vec3::ONE);
    for delta in [Vec3::ZERO, Vec3::X, -Vec3::X * 10.0] {
        assert!(body.sweep_capsule(delta, &capsule).is_none());
    }
    for radius in [-1.0, f32::NAN, f32::INFINITY] {
        assert!(body
            .sweep_capsule(Vec3::X * 6.0, &Capsule::new(capsule.a, capsule.b, radius))
            .is_none());
    }
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for i in 0..3 {
            let mut value = Vec3::ZERO;
            value[i] = bad;
            assert!(body.sweep_capsule(value, &capsule).is_none());
            assert!(body
                .sweep_capsule(Vec3::X * 6.0, &Capsule::new(value, capsule.b, 0.5))
                .is_none());
            assert!(body
                .sweep_capsule(Vec3::X * 6.0, &Capsule::new(capsule.a, value, 0.5))
                .is_none());
            assert!(Aabb::from_min_max(value, Vec3::ONE)
                .sweep_capsule(Vec3::X * 6.0, &capsule)
                .is_none());
            assert!(Aabb::from_min_max(Vec3::ZERO, value)
                .sweep_capsule(Vec3::X * 6.0, &capsule)
                .is_none());
        }
    }
    let inverted = Aabb::from_min_max(Vec3::ONE, Vec3::ZERO);
    assert!(inverted.sweep_capsule(Vec3::X * 6.0, &capsule).is_none());
}

// Independent oracle: split the segment at box-plane crossings. In each
// interval, squared point/box distance is quadratic in the segment parameter.
// This does not enumerate the sweep's active sets or solve its feature roots.
fn distance_sq(body: Aabb, delta: Vec3, capsule: Capsule, t: f64) -> f64 {
    let a = capsule.a.as_dvec3() - delta.as_dvec3() * t;
    let u = capsule.b.as_dvec3() - capsule.a.as_dvec3();
    let min = body.min.as_dvec3();
    let max = body.max.as_dvec3();
    let mut knots = vec![0.0, 1.0];
    for i in 0..3 {
        if u[i] != 0.0 {
            for plane in [min[i], max[i]] {
                let s = (plane - a[i]) / u[i];
                if s > 0.0 && s < 1.0 {
                    knots.push(s);
                }
            }
        }
    }
    knots.sort_by(f64::total_cmp);
    let evaluate = |s: f64| {
        let p = a + u * s;
        (p - p.clamp(min, max)).length_squared()
    };
    let mut best = f64::INFINITY;
    for interval in knots.windows(2) {
        let midpoint = a + u * ((interval[0] + interval[1]) * 0.5);
        let mut c = DVec3::ZERO;
        let mut d = DVec3::ZERO;
        for i in 0..3 {
            let plane = if midpoint[i] < min[i] {
                Some(min[i])
            } else if midpoint[i] > max[i] {
                Some(max[i])
            } else {
                None
            };
            if let Some(plane) = plane {
                c[i] = a[i] - plane;
                d[i] = u[i];
            }
        }
        let s = if d.length_squared() == 0.0 {
            interval[0]
        } else {
            (-c.dot(d) / d.length_squared()).clamp(interval[0], interval[1])
        };
        best = best
            .min(evaluate(s))
            .min(evaluate(interval[0]))
            .min(evaluate(interval[1]));
    }
    best
}

fn oracle(body: Aabb, delta: Vec3, capsule: Capsule) -> Option<f64> {
    let radius_sq = (capsule.radius as f64).powi(2);
    let mut lo = 0.0;
    let mut hi = 1.0;
    // Distance to a convex set under translation is convex; locate its minimum
    // before bisecting the first crossing, including clear endpoint paths.
    for _ in 0..90 {
        let x = (lo * 2.0 + hi) / 3.0;
        let y = (lo + hi * 2.0) / 3.0;
        if distance_sq(body, delta, capsule, x) < distance_sq(body, delta, capsule, y) {
            hi = y;
        } else {
            lo = x;
        }
    }
    let middle = (lo + hi) * 0.5;
    if distance_sq(body, delta, capsule, middle) > radius_sq {
        return None;
    }
    lo = 0.0;
    hi = middle;
    for _ in 0..60 {
        let t = (lo + hi) * 0.5;
        if distance_sq(body, delta, capsule, t) <= radius_sq {
            hi = t;
        } else {
            lo = t;
        }
    }
    Some(hi)
}

#[test]
fn randomized_oblique_capsules_match_independent_distance_bisection() {
    let mut seed = 0xe91c_7b3a_u32;
    let mut next = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) as f32 / (1u32 << 24) as f32
    };
    let mut hits = 0;
    let mut misses = 0;
    for _ in 0..1000 {
        let center = vec3(next() * 2.0 - 1.0, next() * 2.0 - 1.0, next() * 2.0 - 1.0);
        let rotation = Quat::from_euler(
            macroquad::math::EulerRot::XYZ,
            next() * 6.0,
            next() * 6.0,
            next() * 6.0,
        );
        let spine = rotation * Vec3::Y * (0.1 + next() * 2.0);
        let capsule = Capsule::new(center - spine, center + spine, 0.1 + next() * 0.7);
        let body = Aabb::from_center_size(
            vec3(-4.0, next() * 6.0 - 3.0, next() * 6.0 - 3.0),
            vec3(
                0.01 + next() * 0.8,
                0.01 + next() * 2.0,
                0.01 + next() * 2.0,
            ),
        );
        let delta = vec3(8.0, next() - 0.5, next() - 0.5);
        assert!(!capsule.overlaps_aabb(&body));
        match (
            oracle(body, delta, capsule),
            body.sweep_capsule(delta, &capsule),
        ) {
            (Some(expected), Some(hit)) => {
                assert!(
                    (hit.t as f64 - expected).abs() < 2e-6,
                    "expected {expected}, got {hit:?}, {body:?}, {capsule:?}"
                );
                shared_contact(body, delta, capsule, hit);
                hits += 1;
            }
            (None, None) => misses += 1,
            (expected, actual) => {
                panic!("expected {expected:?}, got {actual:?}, {body:?}, {capsule:?}, {delta:?}")
            }
        }
    }
    assert!(hits >= 100 && misses >= 100, "hits {hits}, misses {misses}");
}
