//! Public-API coverage for a translating sphere against a static capsule.

use macroquad::math::{vec3, Capsule, DVec3, Quat, Sphere, SweepHit, Vec3};

fn close(actual: Vec3, expected: Vec3, tolerance: f32) {
    assert!(
        (actual - expected).length() <= tolerance,
        "{actual:?} != {expected:?}"
    );
}

fn closest_segment(point: DVec3, capsule: Capsule) -> DVec3 {
    let a = capsule.a.as_dvec3();
    let axis = capsule.b.as_dvec3() - a;
    let length_sq = axis.length_squared();
    let u = if length_sq == 0.0 {
        0.0
    } else {
        ((point - a).dot(axis) / length_sq).clamp(0.0, 1.0)
    };
    a + axis * u
}

fn shared_contact(body: Sphere, delta: Vec3, capsule: Capsule, hit: SweepHit) {
    assert!((0.0..=1.0).contains(&hit.t), "{hit:?}");
    assert!(hit.point.is_finite() && hit.normal.is_finite());
    assert!((hit.normal.length() - 1.0).abs() < 2e-5, "{hit:?}");
    let center = body.center.as_dvec3() + delta.as_dvec3() * hit.t as f64;
    let core = closest_segment(center, capsule);
    assert!(
        ((center - hit.point.as_dvec3()).length() - body.radius as f64).abs() < 3e-4,
        "{body:?}, {capsule:?}, {hit:?}"
    );
    let surface_core = closest_segment(hit.point.as_dvec3(), capsule);
    assert!(
        ((hit.point.as_dvec3() - surface_core).length() - capsule.radius as f64).abs() < 3e-4,
        "{capsule:?}, {hit:?}"
    );
    close(hit.normal, (center - core).normalize().as_vec3(), 3e-4);
    close(
        hit.point,
        (core + hit.normal.as_dvec3() * capsule.radius as f64).as_vec3(),
        3e-4,
    );
    assert!(delta.dot(hit.normal) < -1e-5, "{delta:?}, {hit:?}");
}

fn post() -> Capsule {
    Capsule::new(-Vec3::Y * 2.0, Vec3::Y * 2.0, 0.5)
}

#[test]
fn continuous_barrel_crossing_has_both_endpoints_clear() {
    let capsule = post();
    let body = Sphere::new(-Vec3::X * 3.0, 0.5);
    let delta = Vec3::X * 6.0;
    assert!(!capsule.overlaps_sphere(&body));
    assert!(!capsule.overlaps_sphere(&body.offset(delta)));
    let hit = body.sweep_capsule(delta, &capsule).unwrap();
    assert!((hit.t - 1.0 / 3.0).abs() < 1e-6);
    assert_eq!(hit.normal, -Vec3::X);
    assert_eq!(hit.point, -Vec3::X * 0.5);
    shared_contact(body, delta, capsule, hit);
}

#[test]
fn thin_high_speed_barrel_crossing_is_not_a_bounding_box_hit() {
    let capsule = Capsule::new(-Vec3::Y * 4.0, Vec3::Y * 4.0, 0.002);
    let body = Sphere::new(vec3(-5000.0, 0.0, 0.001), 0.001);
    let delta = Vec3::X * 10000.0;
    let hit = body.sweep_capsule(delta, &capsule).unwrap();
    let expected = (5000.0 - (0.003_f64 * 0.003 - 0.001 * 0.001).sqrt()) / 10000.0;
    assert!(
        (hit.t as f64 - expected).abs() < 4e-8,
        "{hit:?}, {expected}"
    );
    close(
        hit.normal,
        vec3(-8.0_f32.sqrt() / 3.0, 0.0, 1.0 / 3.0),
        2e-5,
    );
    let miss = Sphere::new(vec3(-5000.0, 0.0, 0.0031), 0.001);
    assert!(miss.sweep_capsule(delta, &capsule).is_none());
}

#[test]
fn both_caps_use_sum_of_radii_and_world_surface_points() {
    let capsule = post();
    for sign in [-1.0, 1.0] {
        let body = Sphere::new(Vec3::Y * 5.0 * sign, 0.5);
        let delta = Vec3::Y * (-6.0 * sign);
        let hit = body.sweep_capsule(delta, &capsule).unwrap();
        assert!((hit.t - 1.0 / 3.0).abs() < 1e-6);
        close(hit.normal, Vec3::Y * sign, 1e-6);
        close(hit.point, Vec3::Y * 2.5 * sign, 1e-6);
        shared_contact(body, delta, capsule, hit);
    }
    let body = Sphere::new(vec3(-3.0, 2.6, 0.0), 0.5);
    let hit = body.sweep_capsule(Vec3::X * 6.0, &capsule).unwrap();
    assert!((hit.t - 2.2 / 6.0).abs() < 1e-6);
    close(hit.normal, vec3(-0.8, 0.6, 0.0), 2e-6);
    shared_contact(body, Vec3::X * 6.0, capsule, hit);
}

#[test]
fn barrel_cap_seams_and_endpoint_reversal_keep_the_earliest_boundary() {
    let capsule = post();
    let reversed = Capsule::new(capsule.b, capsule.a, capsule.radius);
    for sign in [-1.0, 1.0] {
        for offset in [-1e-4, 0.0, 1e-4, 0.4] {
            for axial in [-0.2, 0.0, 0.2] {
                let delta = vec3(6.0, axial * sign, 0.0);
                let body = Sphere::new(vec3(-3.0, (2.0 + offset) * sign, 0.0), 0.5);
                let hit = body.sweep_capsule(delta, &capsule).unwrap();
                let other = body.sweep_capsule(delta, &reversed).unwrap();
                assert!((hit.t - other.t).abs() < 1e-6, "{hit:?}, {other:?}");
                close(hit.normal, other.normal, 2e-5);
                close(hit.point, other.point, 2e-5);
                shared_contact(body, delta, capsule, hit);
                assert!(!overlap_at(body, delta, capsule, hit.t as f64 - 1e-4));
                assert!(overlap_at(body, delta, capsule, hit.t as f64 + 1e-4));
            }
        }
    }
}

#[test]
fn parallel_motion_hits_only_an_exposed_cap_and_resting_travel_is_free() {
    let capsule = post();
    let body = Sphere::new(vec3(0.6, -5.0, 0.0), 0.5);
    let delta = Vec3::Y * 10.0;
    let hit = body.sweep_capsule(delta, &capsule).unwrap();
    assert!((hit.t - 0.22).abs() < 1e-6);
    close(hit.normal, vec3(0.6, -0.8, 0.0), 2e-6);
    shared_contact(body, delta, capsule, hit);
    for x in [1.0, 1.0001] {
        let tangent = Sphere::new(vec3(x, -5.0, 0.0), 0.5);
        assert!(tangent.sweep_capsule(delta, &capsule).is_none());
    }
    let resting = Sphere::new(Vec3::X, 0.5);
    for delta in [Vec3::ZERO, Vec3::X, Vec3::Y * 10.0, Vec3::Z] {
        assert!(resting.sweep_capsule(delta, &capsule).is_none());
    }
}

#[test]
fn future_barrel_and_cap_tangencies_do_not_manufacture_inward_normals() {
    let capsule = post();
    for y in [0.0, 2.0] {
        let body = Sphere::new(vec3(-9.0 / 7.0, y, 1.0), 0.5);
        assert!(body.sweep_capsule(Vec3::X * 130.0, &capsule).is_none());
    }
    let axis = Vec3::Z;
    let capsule = Capsule::new(-axis * 2.0, axis * 2.0, 2.0);
    let rotated_tangent = Sphere::new(vec3(-1.0, 7.0, 0.0), 3.0);
    assert!(rotated_tangent
        .sweep_capsule(vec3(492.0, -369.0, 0.0), &capsule)
        .is_none());
    let almost = Sphere::new(vec3(-3.0, 0.0, 0.999), 0.5);
    let hit = almost.sweep_capsule(Vec3::X * 6.0, &post()).unwrap();
    assert!(hit.t < 0.5);
    shared_contact(almost, Vec3::X * 6.0, post(), hit);
}

#[test]
fn endpoint_at_the_end_of_displacement_is_included() {
    for (body, delta) in [
        (Sphere::new(-Vec3::X * 3.0, 0.5), Vec3::X * 2.0),
        (Sphere::new(-Vec3::Y * 5.0, 0.5), Vec3::Y * 2.0),
    ] {
        let hit = body.sweep_capsule(delta, &post()).unwrap();
        assert_eq!(hit.t, 1.0);
        assert!(body.sweep_capsule(delta * 0.999, &post()).is_none());
        shared_contact(body, delta, post(), hit);
    }
}

#[test]
fn tilted_barrel_endpoint_impacts_survive_segment_reversal_without_a_skin() {
    // Integer orthogonal vectors give an exact radius-five final distance,
    // independent of the implementation's normalized segment arithmetic.
    let body = Sphere::new(vec3(-8.0, 6.0, 0.0), 2.0);
    let delta = vec3(4.0, -3.0, 0.0);
    let short_delta = delta * f32::from_bits(1.0_f32.to_bits() - 1);
    for half_axis in [vec3(9.0, 12.0, 0.0), vec3(30.0, 40.0, 0.0)] {
        for (a, b) in [(-half_axis, half_axis), (half_axis, -half_axis)] {
            let capsule = Capsule::new(a, b, 3.0);
            let hit = body.sweep_capsule(delta, &capsule).unwrap();
            assert_eq!(hit.t, 1.0);
            close(hit.normal, vec3(-0.8, 0.6, 0.0), 1e-6);
            close(hit.point, vec3(-2.4, 1.8, 0.0), 1e-6);
            shared_contact(body, delta, capsule, hit);
            assert!(body.sweep_capsule(short_delta, &capsule).is_none());
        }
    }
}

#[test]
fn barrel_endpoint_slack_does_not_grow_with_canceling_root_terms() {
    // The closest approach is extremely far along a tiny radial movement.
    // Entry is still after the delta (about t=2.5 or t=4.5), so a root-scale
    // tolerance must not turn either path into an endpoint impact.
    let body = Sphere::new(vec3(-1e10, 1000.0, 0.0), 0.0);
    for (a, b) in [(-Vec3::Z, Vec3::Z), (Vec3::Z, -Vec3::Z)] {
        let capsule = Capsule::new(a, b, 1e10);
        for delta in [Vec3::X * 2e-5, Vec3::X * 1.1e-5] {
            assert!(body.sweep_capsule(delta, &capsule).is_none());
        }
    }
}

#[test]
fn shallow_overlap_and_pure_touch_use_world_depth_and_inward_thresholds() {
    for depth in [0.0, 5e-5] {
        let body = Sphere::new(Vec3::X * (1.0 - depth), 0.5);
        for delta in [Vec3::ZERO, Vec3::X, Vec3::Y, -Vec3::X * 1e-5] {
            assert!(
                body.sweep_capsule(delta, &post()).is_none(),
                "{body:?}, {delta:?}"
            );
        }
        let hit = body.sweep_capsule(-Vec3::X * 2e-5, &post()).unwrap();
        assert_eq!(hit.t, 0.0);
        assert_eq!(hit.normal, Vec3::X);
        assert_eq!(hit.point, Vec3::X * 0.5);
    }
}

#[test]
fn deep_overlap_and_containment_return_a_static_capsule_surface_point() {
    for (center, radius, normal, point) in [
        (vec3(0.8, 0.4, 0.0), 0.5, Vec3::X, vec3(0.5, 0.4, 0.0)),
        (vec3(0.1, 0.0, 0.0), 0.0, Vec3::X, Vec3::X * 0.5),
        (vec3(0.0, 2.3, 0.0), 0.5, Vec3::Y, Vec3::Y * 2.5),
        (vec3(0.1, 0.0, 0.0), 10.0, Vec3::X, Vec3::X * 0.5),
    ] {
        let body = Sphere::new(center, radius);
        for delta in [Vec3::ZERO, normal * 4.0, vec3(-3.0, 7.0, 2.0)] {
            let hit = body.sweep_capsule(delta, &post()).unwrap();
            assert_eq!(hit.t, 0.0);
            close(hit.normal, normal, 1e-6);
            close(hit.point, point, 1e-6);
        }
    }
}

#[test]
fn coincident_segment_normals_are_perpendicular_deterministic_and_reversal_invariant() {
    for (axis, expected) in [(Vec3::X, Vec3::Y), (Vec3::Y, Vec3::X), (Vec3::Z, Vec3::X)] {
        let body = Sphere::new(Vec3::ZERO, 0.1);
        for (a, b) in [(-axis, axis), (axis, -axis)] {
            let capsule = Capsule::new(a, b, 0.5);
            let hit = body.sweep_capsule(Vec3::ZERO, &capsule).unwrap();
            assert_eq!(hit.t, 0.0);
            close(hit.normal, expected, 1e-6);
            close(hit.point, expected * 0.5, 1e-6);
        }
    }
    let axis = vec3(1.0, 2.0, 3.0).normalize();
    let expected = (Vec3::X - axis * axis.x).normalize();
    let hit = Sphere::new(Vec3::ZERO, 0.1)
        .sweep_capsule(Vec3::ZERO, &Capsule::new(-axis, axis, 0.5))
        .unwrap();
    close(hit.normal, expected, 2e-6);
    for axis in [
        vec3(1.0, 1.0, 1.0),
        vec3(1.0, 2.0, 3.0),
        vec3(-1.0, 2.0, 3.0),
    ] {
        for scale in [1e-5, 1.0, 1000.0] {
            let capsule = Capsule::new(-axis * scale, axis * scale, 0.5);
            let direction = axis.normalize();
            for center in [Vec3::ZERO, capsule.a, capsule.b] {
                let hit = Sphere::new(center, 0.25)
                    .sweep_capsule(Vec3::ZERO, &capsule)
                    .unwrap();
                assert!(
                    hit.normal.dot(direction).abs() < 2e-6,
                    "{capsule:?}, {center:?}, {hit:?}"
                );
                let core = closest_segment(hit.point.as_dvec3(), capsule);
                assert!(
                    ((hit.point.as_dvec3() - core).length() - 0.5).abs() < 1e-4,
                    "{capsule:?}, {center:?}, {hit:?}"
                );
            }
        }
    }
}

#[test]
fn zero_length_segments_delegate_exactly_to_the_sphere_query() {
    let center = vec3(3.0, -2.0, 5.0);
    for radius in [0.0, 0.5] {
        let capsule = Capsule::new(center, center, radius);
        let solid = Sphere::new(center, radius);
        for body in [
            Sphere::new(center - Vec3::X * 3.0, 0.5),
            Sphere::new(center, 0.0),
            Sphere::new(center + Vec3::X * 0.5, 0.0),
        ] {
            for delta in [Vec3::ZERO, Vec3::X * 6.0, -Vec3::X, Vec3::Y] {
                assert_eq!(
                    body.sweep_capsule(delta, &capsule),
                    body.sweep_sphere(delta, &solid)
                );
            }
        }
    }
}

#[test]
fn short_nonzero_segments_are_not_collapsed_to_an_endpoint() {
    let capsule = Capsule::new(Vec3::ZERO, Vec3::Y * 5e-5, 0.0);
    let body = Sphere::new(vec3(-0.001, 2.5e-5, 0.0), 1e-5);
    let delta = Vec3::X * 0.002;
    let hit = body.sweep_capsule(delta, &capsule).unwrap();
    assert!((hit.t - 0.495).abs() < 1e-6);
    close(hit.normal, -Vec3::X, 1e-6);
    close(hit.point, Vec3::Y * 2.5e-5, 1e-8);
}

#[test]
fn zero_radius_on_either_side_and_isolated_point_crossings_have_defined_behavior() {
    for (body_radius, capsule_radius) in [(0.0, 1.0), (1.0, 0.0)] {
        let capsule = Capsule::new(-Vec3::Y, Vec3::Y, capsule_radius);
        let body = Sphere::new(-Vec3::X * 3.0, body_radius);
        let delta = Vec3::X * 4.0;
        let hit = body.sweep_capsule(delta, &capsule).unwrap();
        assert_eq!(hit.t, 0.5);
        shared_contact(body, delta, capsule, hit);
    }
    let segment = Capsule::new(-Vec3::Y, Vec3::Y, 0.0);
    let point = Sphere::new(-Vec3::X * (3.0 / 7.0), 0.0);
    assert!(point.sweep_capsule(Vec3::X * 11.0, &segment).is_none());
    let coincident = Sphere::new(Vec3::ZERO, 0.0);
    for delta in [Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z] {
        assert!(coincident.sweep_capsule(delta, &segment).is_none());
    }
    let hit = coincident.sweep_capsule(-Vec3::X, &segment).unwrap();
    assert_eq!(hit.t, 0.0);
    assert_eq!(hit.normal, Vec3::X);
    assert_eq!(hit.point, Vec3::ZERO);
}

#[test]
fn arbitrary_up_axes_rotations_and_translations_preserve_world_contacts() {
    for rotation in [
        Quat::IDENTITY,
        Quat::from_rotation_z(1.2),
        Quat::from_euler(macroquad::math::EulerRot::XYZ, 0.4, -0.8, 1.1),
    ] {
        for origin in [Vec3::ZERO, vec3(20.0, -40.0, 13.0)] {
            let base = post();
            let capsule = Capsule::new(
                origin + rotation * base.a,
                origin + rotation * base.b,
                base.radius,
            );
            let body = Sphere::new(origin + rotation * vec3(-3.0, 0.3, 0.6), 0.5);
            let delta = rotation * Vec3::X * 6.0;
            let hit = body.sweep_capsule(delta, &capsule).unwrap();
            assert!((hit.t - 2.2 / 6.0).abs() < 2e-6, "{hit:?}");
            close(hit.normal, rotation * vec3(-0.8, 0.0, 0.6), 6e-6);
            close(hit.point, origin + rotation * vec3(-0.4, 0.3, 0.3), 6e-6);
            shared_contact(body, delta, capsule, hit);
        }
    }
}

#[test]
fn misses_and_invalid_inputs_are_rejected_including_during_initial_overlap() {
    let body = Sphere::new(-Vec3::X * 3.0, 0.5);
    for delta in [Vec3::ZERO, -Vec3::X * 6.0, Vec3::X, Vec3::Z * 10.0] {
        assert!(body.sweep_capsule(delta, &post()).is_none());
    }
    for radius in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(body
            .sweep_capsule(Vec3::X * 6.0, &Capsule::new(post().a, post().b, radius))
            .is_none());
        assert!(Sphere::new(Vec3::ZERO, radius)
            .sweep_capsule(Vec3::ZERO, &post())
            .is_none());
    }
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for axis in 0..3 {
            let mut broken = Vec3::ZERO;
            broken[axis] = invalid;
            assert!(body.sweep_capsule(broken, &post()).is_none());
            assert!(Sphere::new(Vec3::ZERO, 0.5)
                .sweep_capsule(broken, &post())
                .is_none());
            assert!(Sphere::new(broken, 0.5)
                .sweep_capsule(Vec3::ZERO, &post())
                .is_none());
            assert!(body
                .sweep_capsule(Vec3::X * 6.0, &Capsule::new(broken, post().b, 0.5))
                .is_none());
            assert!(body
                .sweep_capsule(Vec3::X * 6.0, &Capsule::new(post().a, broken, 0.5))
                .is_none());
        }
    }
    let huge = Capsule::new(-Vec3::Y, Vec3::Y, f32::MAX);
    assert!(Sphere::new(Vec3::ZERO, f32::MAX)
        .sweep_capsule(Vec3::ZERO, &huge)
        .is_none());
}

// Independent f64 static distance oracle, with no engine shape query.
fn overlap_at(body: Sphere, delta: Vec3, capsule: Capsule, t: f64) -> bool {
    let center = body.center.as_dvec3() + delta.as_dvec3() * t;
    let reach = body.radius as f64 + capsule.radius as f64;
    (center - closest_segment(center, capsule)).length_squared() <= reach * reach
}

#[test]
fn varied_paths_match_independently_sampled_and_bisected_overlap_boundaries() {
    let mut state = 0x98a6_5432_u32;
    let mut random = || {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        (state >> 8) as f32 / (1_u32 << 24) as f32
    };
    for case in 0..320 {
        let center = vec3(random(), random(), random()) * 10.0 - Vec3::splat(5.0);
        let axis = (vec3(random(), random(), random()) - Vec3::splat(0.5)).normalize();
        let half_length = 0.01 + random() * 3.0;
        let capsule = Capsule::new(
            center - axis * half_length,
            center + axis * half_length,
            0.2 + random(),
        );
        let direction = (vec3(random(), random(), random()) - Vec3::splat(0.5)).normalize();
        let target = center + axis * ((random() - 0.5) * 2.5 * half_length);
        let body = Sphere::new(target - direction * 6.0, 0.1 + random());
        let delta = direction * 12.0;
        assert!(!overlap_at(body, delta, capsule, 0.0));
        let sample = (1..=2048)
            .find(|step| overlap_at(body, delta, capsule, *step as f64 / 2048.0))
            .unwrap();
        let mut low = (sample - 1) as f64 / 2048.0;
        let mut high = sample as f64 / 2048.0;
        for _ in 0..48 {
            let middle = (low + high) * 0.5;
            if overlap_at(body, delta, capsule, middle) {
                high = middle;
            } else {
                low = middle;
            }
        }
        let hit = body.sweep_capsule(delta, &capsule).unwrap();
        assert!(
            (hit.t as f64 - high).abs() < 3e-5,
            "case {case}, {hit:?}, expected {high}"
        );
        shared_contact(body, delta, capsule, hit);
        let reversed = Capsule::new(capsule.b, capsule.a, capsule.radius);
        let other = body.sweep_capsule(delta, &reversed).unwrap();
        assert!((hit.t - other.t).abs() < 2e-6);
        close(hit.normal, other.normal, 2e-5);
        let cross = axis.cross(direction);
        if cross.length_squared() > 0.01 {
            let miss = Sphere::new(
                target + cross.normalize() * (body.radius + capsule.radius + 0.2) - direction * 6.0,
                body.radius,
            );
            assert!((0..=512).all(|step| !overlap_at(miss, delta, capsule, step as f64 / 512.0)));
            assert!(miss.sweep_capsule(delta, &capsule).is_none());
        }
    }
}
