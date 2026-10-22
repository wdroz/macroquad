//! Public-API coverage of a translating sphere against a static sphere.

use macroquad::math::{vec3, Sphere, SweepHit, Vec3};

fn assert_vec(actual: Vec3, expected: Vec3, tolerance: f32) {
    assert!(
        (actual - expected).length() <= tolerance,
        "actual {actual:?}, expected {expected:?}"
    );
}

fn assert_new_contact(body: Sphere, delta: Vec3, solid: Sphere, hit: SweepHit) {
    assert!(hit.t.is_finite() && (0.0..=1.0).contains(&hit.t));
    assert!(hit.point.is_finite() && hit.normal.is_finite());
    assert!((hit.normal.length() - 1.0).abs() < 2e-5, "{hit:?}");
    let at = body.center + delta * hit.t;
    assert!(
        ((hit.point - at).length() - body.radius).abs() < 3e-4,
        "point outside moving sphere surface: {hit:?}, {body:?}"
    );
    assert!(
        ((hit.point - solid.center).length() - solid.radius).abs() < 3e-4,
        "point outside static sphere surface: {hit:?}, {solid:?}"
    );
    assert_vec(hit.normal, (at - solid.center).normalize(), 3e-4);
    assert_vec(hit.point, solid.center + hit.normal * solid.radius, 3e-4);
    assert!(delta.dot(hit.normal) < -1e-5, "{hit:?}, {delta:?}");
}

#[test]
fn continuous_hit_catches_a_crossing_with_both_endpoints_clear() {
    let solid = Sphere::new(vec3(1.0, 2.0, 3.0), 0.25);
    let body = Sphere::new(vec3(-2.0, 2.0, 3.0), 0.5);
    let delta = Vec3::X * 6.0;
    assert!(!body.overlaps(&solid));
    assert!(!body.offset(delta).overlaps(&solid));
    let hit = body.sweep_sphere(delta, &solid).unwrap();
    assert_eq!(hit.t, 0.375);
    assert_eq!(hit.normal, -Vec3::X);
    assert_eq!(hit.point, vec3(0.75, 2.0, 3.0));
    assert_new_contact(body, delta, solid, hit);
}

#[test]
fn all_world_axes_and_motion_signs_have_the_same_conventions() {
    let solid = Sphere::new(vec3(5.0, -3.0, 2.0), 0.75);
    for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
        for sign in [-1.0, 1.0] {
            let direction = axis * sign;
            let body = Sphere::new(solid.center - direction * 3.0, 0.25);
            let delta = direction * 4.0;
            let hit = body.sweep_sphere(delta, &solid).unwrap();
            assert_eq!(hit.t, 0.5);
            assert_eq!(hit.normal, -direction);
            assert_eq!(hit.point, solid.center - direction * solid.radius);
            assert_new_contact(body, delta, solid, hit);
        }
    }
}

#[test]
fn contact_at_the_end_of_the_displacement_is_included() {
    let solid = Sphere::new(Vec3::ZERO, 0.75);
    let body = Sphere::new(-Vec3::X * 3.0, 0.25);
    let hit = body.sweep_sphere(Vec3::X * 2.0, &solid).unwrap();
    assert_eq!(hit.t, 1.0);
    assert_new_contact(body, Vec3::X * 2.0, solid, hit);
    assert!(body.sweep_sphere(Vec3::X * 1.999, &solid).is_none());
}

#[test]
fn off_center_graze_uses_the_sum_of_radii_and_a_shared_surface_point() {
    let solid = Sphere::new(vec3(3.0, -2.0, 5.0), 0.6);
    let body = Sphere::new(solid.center + vec3(-3.0, 0.6, 0.0), 0.4);
    let delta = Vec3::X * 6.0;
    let hit = body.sweep_sphere(delta, &solid).unwrap();
    assert!((hit.t - 2.2 / 6.0).abs() < 1e-6);
    assert_vec(hit.normal, vec3(-0.8, 0.6, 0.0), 2e-6);
    assert_vec(hit.point, solid.center + vec3(-0.48, 0.36, 0.0), 2e-6);
    assert_new_contact(body, delta, solid, hit);
    assert!(!body.offset(delta * (hit.t - 0.001)).overlaps(&solid));
    assert!(body.offset(delta * (hit.t + 0.001)).overlaps(&solid));

    let miss = Sphere::new(solid.center + vec3(-3.0, 0.8, 0.8), 0.4);
    assert!(miss.sweep_sphere(delta, &solid).is_none());
}

#[test]
fn oblique_motion_applies_every_component_at_the_impact_time() {
    let solid = Sphere::new(vec3(13.0, -7.0, 4.0), 0.75);
    let normal = vec3(-0.8, 0.48, 0.36);
    let delta = vec3(6.0, 0.5, -0.3);
    let body = Sphere::new(solid.center + normal * 1.25 - delta * 0.4, 0.5);
    let hit = body.sweep_sphere(delta, &solid).unwrap();
    assert!((hit.t - 0.4).abs() < 2e-6, "{hit:?}");
    assert_vec(hit.normal, normal, 3e-6);
    assert_vec(hit.point, solid.center + normal * solid.radius, 3e-6);
    assert_new_contact(body, delta, solid, hit);
}

#[test]
fn a_future_tangent_is_not_an_inward_collision() {
    // A rounded entry fraction must not manufacture inward motion at a true
    // tangent, even when a high speed amplifies tiny radial residue.
    let fractional = Sphere::new(vec3(-9.0 / 7.0, 1.0, 0.0), 0.5);
    let tangent = Sphere::new(Vec3::ZERO, 0.5);
    assert!(fractional.sweep_sphere(Vec3::X * 130.0, &tangent).is_none());
    // The supporting line is exactly tangent at (3,4,0), t=1/123. A
    // closest-point projection alone rounds its squared distance below 25.
    let rotated = Sphere::new(vec3(-1.0, 7.0, 0.0), 3.0);
    assert!(rotated
        .sweep_sphere(vec3(492.0, -369.0, 0.0), &Sphere::new(Vec3::ZERO, 2.0))
        .is_none());
    let solid = Sphere::new(Vec3::ZERO, 0.75);
    for y in [1.0, 1.0001] {
        let body = Sphere::new(vec3(-3.0, y, 0.0), 0.25);
        assert!(body.sweep_sphere(Vec3::X * 6.0, &solid).is_none());
    }
    let entering = Sphere::new(vec3(-3.0, 0.999, 0.0), 0.25);
    let hit = entering.sweep_sphere(Vec3::X * 6.0, &solid).unwrap();
    assert!(hit.t < 0.5);
    assert_new_contact(entering, Vec3::X * 6.0, solid, hit);
}

#[test]
fn touching_spheres_only_report_motion_strictly_inward() {
    let solid = Sphere::new(Vec3::ZERO, 0.75);
    let body = Sphere::new(-Vec3::X, 0.25);
    for delta in [Vec3::ZERO, -Vec3::X, Vec3::Y, Vec3::Z] {
        assert!(body.sweep_sphere(delta, &solid).is_none(), "{delta:?}");
    }
    let contact = body.contact(&solid).unwrap();
    let hit = body.sweep_sphere(Vec3::X, &solid).unwrap();
    assert_eq!(hit.t, 0.0);
    assert_eq!(hit.normal, contact.normal);
    assert_eq!(hit.point, contact.point);

    for speed in [0.5e-5, 1e-5] {
        assert!(body.sweep_sphere(Vec3::X * speed, &solid).is_none());
    }
    assert_eq!(body.sweep_sphere(Vec3::X * 2e-5, &solid).unwrap().t, 0.0);
}

#[test]
fn shallow_overlap_uses_world_depth_and_the_same_inward_threshold() {
    let solid = Sphere::new(Vec3::ZERO, 0.75);
    let body = Sphere::new(vec3(-1.0 + 5e-5, 0.0, 0.0), 0.25);
    let contact = body.contact(&solid).unwrap();
    assert!(contact.penetration > 0.0 && contact.penetration < 1e-4);
    for delta in [Vec3::ZERO, -Vec3::X, Vec3::Y, Vec3::X * 1e-5] {
        assert!(body.sweep_sphere(delta, &solid).is_none(), "{delta:?}");
    }
    let hit = body.sweep_sphere(Vec3::X * 2e-5, &solid).unwrap();
    assert_eq!(hit.t, 0.0);
    assert_eq!(hit.normal, contact.normal);
    assert_eq!(hit.point, contact.point);
}

#[test]
fn deep_overlap_and_containment_return_the_existing_contact_for_any_motion() {
    let solid = Sphere::new(vec3(3.0, -2.0, 5.0), 1.0);
    let bodies = [
        Sphere::new(solid.center - Vec3::X, 0.5),
        Sphere::new(solid.center + Vec3::Y * 0.2, 0.1),
        Sphere::new(solid.center + Vec3::Z * 0.2, 3.0),
        Sphere::new(solid.center, 0.25),
        Sphere::new(solid.center, 0.0),
    ];
    for body in bodies {
        let contact = body.contact(&solid).unwrap();
        assert!(contact.penetration > 1e-4);
        for delta in [Vec3::ZERO, vec3(-10.0, 7.0, 3.0), contact.normal] {
            let hit = body.sweep_sphere(delta, &solid).unwrap();
            assert_eq!(hit.t, 0.0);
            assert_eq!(hit.normal, contact.normal);
            assert_eq!(hit.point, contact.point);
        }
    }
}

#[test]
fn zero_radius_on_either_side_still_has_a_valid_surface_hit() {
    for (moving_radius, static_radius) in [(0.0, 1.0), (1.0, 0.0)] {
        let body = Sphere::new(-Vec3::X * 3.0, moving_radius);
        let solid = Sphere::new(Vec3::ZERO, static_radius);
        let hit = body.sweep_sphere(Vec3::X * 4.0, &solid).unwrap();
        assert_eq!(hit.t, 0.5);
        assert_eq!(hit.normal, -Vec3::X);
        assert_eq!(hit.point, -Vec3::X * static_radius);
        assert_new_contact(body, Vec3::X * 4.0, solid, hit);
    }
}

#[test]
fn isolated_point_crossing_has_no_inward_normal() {
    let solid = Sphere::new(Vec3::ZERO, 0.0);
    let body = Sphere::new(-Vec3::X, 0.0);
    assert!(body.sweep_sphere(Vec3::X * 2.0, &solid).is_none());
    assert!(body.sweep_sphere(Vec3::X, &solid).is_none());

    // Rounding the entry time must not turn cancellation residue into a
    // radial direction and invent an inward normal for an isolated point.
    let fractional = Sphere::new(vec3(-3.0 / 7.0, 0.0, 0.0), 0.0);
    assert!(fractional.sweep_sphere(Vec3::X * 11.0, &solid).is_none());
}

#[test]
fn coincident_points_keep_the_contact_positive_x_convention() {
    let point = Sphere::new(vec3(3.0, -2.0, 5.0), 0.0);
    for delta in [Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z, -Vec3::X * 1e-5] {
        assert!(point.sweep_sphere(delta, &point).is_none(), "{delta:?}");
    }
    let hit = point.sweep_sphere(-Vec3::X, &point).unwrap();
    assert_eq!(hit.t, 0.0);
    assert_eq!(hit.normal, Vec3::X);
    assert_eq!(hit.point, point.center);
}

#[test]
fn separated_spheres_without_an_entry_miss() {
    let solid = Sphere::new(Vec3::ZERO, 0.75);
    let body = Sphere::new(-Vec3::X * 3.0, 0.25);
    for delta in [Vec3::ZERO, Vec3::X, -Vec3::X * 10.0, Vec3::Y * 10.0] {
        assert!(body.sweep_sphere(delta, &solid).is_none(), "{delta:?}");
    }
}

#[test]
fn negative_and_nonfinite_inputs_miss_even_during_initial_overlap() {
    let body = Sphere::new(-Vec3::X * 3.0, 0.25);
    let solid = Sphere::new(Vec3::ZERO, 0.75);
    for radius in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let invalid = Sphere::new(Vec3::ZERO, radius);
        assert!(body.sweep_sphere(Vec3::X * 6.0, &invalid).is_none());
        assert!(invalid.sweep_sphere(Vec3::X, &solid).is_none());
    }
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for axis in 0..3 {
            let mut invalid = Vec3::ZERO;
            invalid[axis] = bad;
            assert!(body.sweep_sphere(invalid, &solid).is_none());
            assert!(solid.sweep_sphere(invalid, &solid).is_none());
            assert!(body
                .sweep_sphere(Vec3::X * 6.0, &Sphere::new(invalid, 0.75))
                .is_none());
            assert!(Sphere::new(invalid, 0.25)
                .sweep_sphere(Vec3::X * 6.0, &solid)
                .is_none());
        }
    }
}

#[test]
fn finite_inputs_with_unrepresentable_contact_arithmetic_miss() {
    let point = Sphere::new(Vec3::ZERO, 0.0);
    let overflowing_reach = Sphere::new(Vec3::ZERO, f32::MAX);
    assert!(overflowing_reach
        .sweep_sphere(Vec3::ZERO, &overflowing_reach)
        .is_none());
    assert!(Sphere::new(Vec3::X * f32::MAX, 1.0)
        .sweep_sphere(-Vec3::X, &Sphere::new(-Vec3::X * f32::MAX, 1.0))
        .is_none());

    // The existing initial contact would produce an infinite world point.
    let huge_solid = Sphere::new(Vec3::X * f32::MAX, f32::MAX);
    let huge_point = Sphere::new(huge_solid.center, 0.0);
    assert!(huge_point
        .contact(&huge_solid)
        .unwrap()
        .point
        .x
        .is_infinite());
    assert!(huge_point.sweep_sphere(Vec3::ZERO, &huge_solid).is_none());

    // f32 squared lengths overflow although the inputs and radius sum are finite.
    let body = Sphere::new(-Vec3::X * 1e20, 1e20);
    let solid = Sphere::new(Vec3::X * 1e20, 1e20);
    assert!(body.sweep_sphere(Vec3::ZERO, &solid).is_none());
    assert!(body.sweep_sphere(Vec3::X, &point).is_none());
}

// Independent static overlap oracle, using f64 and no engine collision query.
fn overlap_at(body: Sphere, delta: Vec3, solid: Sphere, t: f64) -> bool {
    let distance_squared: f64 = (0..3)
        .map(|axis| {
            let gap = body.center[axis] as f64 + delta[axis] as f64 * t - solid.center[axis] as f64;
            gap * gap
        })
        .sum();
    let reach = body.radius as f64 + solid.radius as f64;
    distance_squared <= reach * reach
}

#[test]
fn varied_trajectories_match_independent_dense_overlap_boundaries() {
    let mut state = 0x7be1_0493_u32;
    let mut random = || {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        (state >> 8) as f32 / (1_u32 << 24) as f32
    };
    for _ in 0..240 {
        let solid = Sphere::new(vec3(random(), random(), random()) * 10.0, 0.2 + random());
        let radius = 0.1 + random();
        let reach = radius + solid.radius;
        let direction = (vec3(random(), random(), random()) - Vec3::splat(0.5)).normalize();
        let cross = direction.cross(Vec3::Y);
        let side = if cross.length_squared() > 0.01 {
            cross.normalize()
        } else {
            direction.cross(Vec3::X).normalize()
        };
        let offset = side * ((random() - 0.5) * 1.5 * reach);
        let body = Sphere::new(solid.center - direction * 5.0 + offset, radius);
        let delta = direction * 10.0;
        assert!(!overlap_at(body, delta, solid, 0.0));
        let sample = (1..=1024)
            .find(|step| overlap_at(body, delta, solid, *step as f64 / 1024.0))
            .expect("the line passes well inside the sum of radii");
        let mut low = (sample - 1) as f64 / 1024.0;
        let mut high = sample as f64 / 1024.0;
        for _ in 0..50 {
            let middle = (low + high) * 0.5;
            if overlap_at(body, delta, solid, middle) {
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

        let miss = Sphere::new(
            solid.center - direction * 5.0 + side * (reach + 0.25 + random()),
            radius,
        );
        assert!((0..=1024).all(|step| !overlap_at(miss, delta, solid, step as f64 / 1024.0)));
        assert!(miss.sweep_sphere(delta, &solid).is_none());
    }
}
