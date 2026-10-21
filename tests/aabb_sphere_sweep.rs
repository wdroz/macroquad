//! Public-API coverage of the complete translating AABB against a static sphere.

use macroquad::math::{vec3, Aabb, Sphere, SweepHit, Vec3};

fn assert_vec(actual: Vec3, expected: Vec3, tolerance: f32) {
    assert!(
        (actual - expected).length() <= tolerance,
        "actual {actual:?}, expected {expected:?}"
    );
}

fn assert_new_contact(body: Aabb, delta: Vec3, solid: Sphere, hit: SweepHit) {
    assert!(hit.t.is_finite() && (0.0..=1.0).contains(&hit.t));
    assert!((hit.normal.length() - 1.0).abs() < 2e-5, "{hit:?}");
    assert!(hit.point.is_finite());
    let impact = body.offset(delta * hit.t);
    assert!(impact.expanded(3e-4).contains(hit.point), "{hit:?}");
    let from_center = hit.point - solid.center;
    assert!(
        (from_center.length() - solid.radius).abs() < 3e-4,
        "point outside sphere surface: {hit:?}, {solid:?}"
    );
    if solid.radius > 0.0 {
        assert_vec(hit.normal, from_center.normalize(), 3e-4);
    }
    assert!(delta.dot(hit.normal) < 0.0);
}

#[test]
fn thin_body_crosses_the_sphere_with_both_endpoints_clear() {
    let solid = Sphere::new(vec3(1.0, 2.0, 3.0), 0.2);
    let body = Aabb::from_center_size(vec3(-2.0, 2.0, 3.0), vec3(0.02, 2.0, 2.0));
    let delta = vec3(6.0, 0.0, 0.0);
    assert!(!solid.overlaps_aabb(&body));
    assert!(!solid.overlaps_aabb(&body.offset(delta)));
    let hit = body.sweep_sphere(delta, &solid).unwrap();
    assert!((hit.t - (3.0 - 0.01 - 0.2) / 6.0).abs() < 1e-6);
    assert_vec(hit.normal, -Vec3::X, 1e-6);
    assert_vec(hit.point, solid.center - Vec3::X * 0.2, 1e-6);
    assert_new_contact(body, delta, solid, hit);
}

#[test]
fn tall_body_hits_where_a_center_sphere_and_ray_miss() {
    let solid = Sphere::new(vec3(0.0, 1.2, 0.0), 0.2);
    let body = Aabb::from_center_size(vec3(-3.0, 0.0, 0.0), vec3(0.1, 3.0, 0.5));
    let delta = Vec3::X * 6.0;
    assert!(solid.ray_intersect(body.center(), delta).is_none());
    let center_path = Aabb::from_center_size(body.center(), Vec3::splat(0.1));
    assert!(center_path.sweep_sphere(delta, &solid).is_none());
    let hit = body.sweep_sphere(delta, &solid).unwrap();
    assert!((hit.t - 2.75 / 6.0).abs() < 1e-6);
    assert_new_contact(body, delta, solid, hit);
}

#[test]
fn corner_graze_has_the_sphere_outward_normal_and_shared_world_point() {
    let solid = Sphere::new(Vec3::ZERO, 1.0);
    let body = Aabb::from_min_max(vec3(-3.0, 0.4, 0.3), vec3(-2.9, 1.0, 1.0));
    let delta = Vec3::X * 6.0;
    let point = vec3(-0.75_f32.sqrt(), 0.4, 0.3);
    let hit = body.sweep_sphere(delta, &solid).unwrap();
    assert!((hit.t - (2.9 + point.x) / 6.0).abs() < 1e-6);
    assert_vec(hit.point, point, 2e-6);
    assert_vec(hit.normal, point, 2e-6);
    assert_new_contact(body, delta, solid, hit);
    assert!(!solid.overlaps_aabb(&body.offset(delta * (hit.t - 0.001))));
    assert!(solid.overlaps_aabb(&body.offset(delta * (hit.t + 0.001))));
}

#[test]
fn edge_graze_does_not_use_the_spheres_bounding_box() {
    let solid = Sphere::new(Vec3::ZERO, 1.0);
    let body = Aabb::from_min_max(vec3(-3.0, 0.6, -1.0), vec3(-2.9, 1.0, 1.0));
    let delta = Vec3::X * 6.0;
    let hit = body.sweep_sphere(delta, &solid).unwrap();
    assert!((hit.t - 2.1 / 6.0).abs() < 1e-6);
    assert_vec(hit.point, vec3(-0.8, 0.6, 0.0), 2e-6);
    assert_new_contact(body, delta, solid, hit);
    let miss = Aabb::from_min_max(vec3(-3.0, 0.8, 0.8), vec3(-2.9, 1.0, 1.0));
    assert!(miss.sweep_sphere(delta, &solid).is_none());
}

#[test]
fn moving_frame_translation_applies_every_component_at_the_impact_time() {
    let solid = Sphere::new(vec3(13.0, -7.0, 4.0), 1.0);
    let normal = vec3(-0.8, 0.48, 0.36);
    let point = solid.center + normal;
    let delta = vec3(6.0, 0.5, -0.3);
    let impact = Aabb::from_min_max(point - Vec3::X * 0.1, point + vec3(0.0, 0.8, 0.7));
    let body = impact.offset(-delta * 0.4);
    let hit = body.sweep_sphere(delta, &solid).unwrap();
    assert!((hit.t - 0.4).abs() < 2e-6, "{hit:?}");
    assert_vec(hit.point, point, 3e-6);
    assert_vec(hit.normal, normal, 3e-6);
    assert_new_contact(body, delta, solid, hit);

    // The reciprocal query's point remains on the original box. In the
    // caller's frame this contact must have moved by delta * t, not -delta*t.
    let relative = solid.sweep_aabb(-delta, &body).unwrap();
    assert_eq!(hit.t, relative.t);
    assert_eq!(hit.normal, -relative.normal);
    assert_vec(hit.point, relative.point + delta * relative.t, 2e-6);
    assert!((hit.point - relative.point).length() > 2.0);
}

#[test]
fn all_world_axes_and_motion_signs_keep_the_same_conventions() {
    let axes = [Vec3::X, Vec3::Y, Vec3::Z];
    let solid = Sphere::new(vec3(5.0, -3.0, 2.0), 0.5);
    for axis in axes {
        for sign in [-1.0, 1.0] {
            let direction = axis * sign;
            let body = Aabb::from_center_size(solid.center - direction * 3.0, Vec3::splat(0.5));
            let delta = direction * 6.0;
            let hit = body.sweep_sphere(delta, &solid).unwrap();
            assert!((hit.t - 2.25 / 6.0).abs() < 1e-6);
            assert_vec(hit.normal, -direction, 1e-6);
            assert_vec(hit.point, solid.center - direction * solid.radius, 1e-6);
            assert_new_contact(body, delta, solid, hit);
        }
    }
}

#[test]
fn touching_body_can_move_away_or_tangentially_but_reports_motion_inward() {
    let solid = Sphere::new(Vec3::ZERO, 1.0);
    let body = Aabb::from_min_max(vec3(-2.0, -0.5, -0.5), vec3(-1.0, 0.5, 0.5));
    for delta in [-Vec3::X, Vec3::Y, Vec3::Z, Vec3::ZERO] {
        assert!(body.sweep_sphere(delta, &solid).is_none(), "{delta:?}");
    }
    let hit = body.sweep_sphere(Vec3::X, &solid).unwrap();
    assert_eq!(hit.t, 0.0);
    assert_eq!(hit.normal, -Vec3::X);
    assert_eq!(hit.point, -Vec3::X);
}

#[test]
fn shallow_overlap_keeps_the_existing_world_distance_tolerance() {
    let solid = Sphere::new(Vec3::ZERO, 1.0);
    let body = Aabb::from_min_max(vec3(-2.0, -0.5, -0.5), vec3(-1.0 + 5e-5, 0.5, 0.5));
    assert!(body.sweep_sphere(-Vec3::X, &solid).is_none());
    assert!(body.sweep_sphere(Vec3::Y, &solid).is_none());
    assert!(body.sweep_sphere(Vec3::ZERO, &solid).is_none());
    assert_eq!(body.sweep_sphere(Vec3::X, &solid).unwrap().t, 0.0);
}

#[test]
fn initial_overlap_and_containment_keep_the_reciprocal_contact_point() {
    let solid = Sphere::new(vec3(3.0, -2.0, 5.0), 1.0);
    let bodies = [
        Aabb::from_center_size(solid.center - Vec3::X, Vec3::ONE),
        Aabb::from_center_size(solid.center + Vec3::X * 0.2, Vec3::splat(0.4)),
        Aabb::from_center_size(solid.center, Vec3::splat(4.0)),
    ];
    for body in bodies {
        let contact = solid.contact_aabb(&body).unwrap();
        for delta in [Vec3::ZERO, vec3(-10.0, 7.0, 3.0)] {
            let hit = body.sweep_sphere(delta, &solid).unwrap();
            assert_eq!(hit.t, 0.0);
            assert_eq!(hit.normal, -contact.normal);
            assert_eq!(hit.point, contact.point);
        }
    }
}

#[test]
fn degenerate_box_and_zero_radius_sphere_can_hit() {
    let solid = Sphere::new(Vec3::ZERO, 1.0);
    let body = Aabb::from_min_max(-Vec3::X * 2.0, -Vec3::X * 2.0);
    let hit = body.sweep_sphere(Vec3::X * 4.0, &solid).unwrap();
    assert_eq!(hit.t, 0.25);
    assert_new_contact(body, Vec3::X * 4.0, solid, hit);

    let point = Sphere::new(Vec3::ZERO, 0.0);
    let body = Aabb::from_min_max(vec3(-2.0, -1.0, -1.0), vec3(-1.0, 1.0, 1.0));
    let hit = body.sweep_sphere(Vec3::X * 2.0, &point).unwrap();
    assert_eq!(hit.t, 0.5);
    assert_eq!(hit.normal, -Vec3::X);
    assert_eq!(hit.point, Vec3::ZERO);
}

#[test]
fn invalid_inputs_and_motion_short_of_impact_miss() {
    let solid = Sphere::new(Vec3::ZERO, 1.0);
    let body = Aabb::from_center_size(-Vec3::X * 3.0, Vec3::ONE);
    assert!(body.sweep_sphere(Vec3::X, &solid).is_none());
    assert!(body.sweep_sphere(-Vec3::X * 10.0, &solid).is_none());
    for radius in [-1.0, f32::NAN, f32::INFINITY] {
        assert!(body
            .sweep_sphere(Vec3::X * 6.0, &Sphere::new(Vec3::ZERO, radius))
            .is_none());
    }
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for axis in 0..3 {
            let mut invalid = Vec3::ZERO;
            invalid[axis] = bad;
            assert!(body.sweep_sphere(invalid, &solid).is_none());
            assert!(body
                .sweep_sphere(Vec3::X * 6.0, &Sphere::new(invalid, 1.0))
                .is_none());
            assert!(Aabb::from_min_max(invalid, Vec3::ONE)
                .sweep_sphere(Vec3::X, &solid)
                .is_none());
            assert!(Aabb::from_min_max(-Vec3::ONE, invalid)
                .sweep_sphere(Vec3::X, &solid)
                .is_none());
        }
    }
    assert!(Aabb::from_min_max(Vec3::ONE, Vec3::ZERO)
        .sweep_sphere(Vec3::X, &solid)
        .is_none());
}

// Independent distance-to-AABB oracle: no engine overlap or reciprocal sweep.
fn distance_squared(body: Aabb, delta: Vec3, sphere: Sphere, t: f64) -> f64 {
    (0..3)
        .map(|axis| {
            let min = body.min[axis] as f64 + delta[axis] as f64 * t;
            let max = body.max[axis] as f64 + delta[axis] as f64 * t;
            let center = sphere.center[axis] as f64;
            let gap = (min - center).max(center - max).max(0.0);
            gap * gap
        })
        .sum()
}

#[test]
fn randomized_hits_match_independent_static_distance_bisection() {
    let mut state = 0xa491_5891_u32;
    let mut random = || {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        (state >> 8) as f32 / (1_u32 << 24) as f32
    };
    for _ in 0..300 {
        let solid = Sphere::new(vec3(random(), random(), random()) * 10.0, 0.2 + random());
        let direction = (vec3(random(), random(), random()) - Vec3::splat(0.5)).normalize();
        let size = vec3(random(), random(), random()) + Vec3::splat(0.02);
        let body = Aabb::from_center_size(solid.center - direction * 5.0, size);
        let delta = direction * 10.0;
        let radius_squared = (solid.radius as f64).powi(2);
        let mut low = 0.0;
        let mut high = 0.5;
        assert!(distance_squared(body, delta, solid, low) > radius_squared);
        assert!(distance_squared(body, delta, solid, high) < radius_squared);
        for _ in 0..55 {
            let middle = (low + high) * 0.5;
            if distance_squared(body, delta, solid, middle) <= radius_squared {
                high = middle;
            } else {
                low = middle;
            }
        }
        let hit = body.sweep_sphere(delta, &solid).unwrap();
        assert!(
            (hit.t as f64 - high).abs() < 3e-5,
            "{hit:?}, expected {high}"
        );
        assert_new_contact(body, delta, solid, hit);

        // Entire trajectory is below this sphere's lowest Y, guaranteeing a
        // miss independently of the rounded Minkowski boundary.
        let y = body.max.y.max(body.max.y + delta.y) + solid.radius + 1.0;
        let miss = Sphere::new(vec3(solid.center.x, y, solid.center.z), solid.radius);
        assert!(body.sweep_sphere(delta, &miss).is_none());
    }
}
