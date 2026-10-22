//! Public checked bounds: containment uses exact comparisons, never an epsilon.

use macroquad::math::{vec3, Aabb, Capsule, DQuat, DVec3, Obb, Quat, Sphere, Vec3};

fn finite_ordered(bounds: Aabb) {
    assert!(
        bounds.min.is_finite() && bounds.max.is_finite(),
        "{bounds:?}"
    );
    assert!(bounds.min.cmple(bounds.max).all(), "{bounds:?}");
}

fn contains_f64(bounds: Aabb, point: DVec3) {
    assert!(
        point.cmpge(bounds.min.as_dvec3()).all() && point.cmple(bounds.max.as_dvec3()).all(),
        "{bounds:?} does not contain {point:?}"
    );
}

fn unit_rotation(rotation: Quat) -> Quat {
    rotation * (1.0 / rotation.length())
}

fn check_obb_corners(box_: Obb) {
    let bounds = box_.bounding_aabb().unwrap();
    finite_ordered(bounds);
    let rotation = unit_rotation(box_.rotation);
    let q = rotation.to_array().map(f64::from);
    let exact_rotation = DQuat::from_xyzw(q[0], q[1], q[2], q[3]);
    for bits in 0..8 {
        let local = vec3(
            if bits & 1 != 0 {
                box_.half_extents.x
            } else {
                -box_.half_extents.x
            },
            if bits & 2 != 0 {
                box_.half_extents.y
            } else {
                -box_.half_extents.y
            },
            if bits & 4 != 0 {
                box_.half_extents.z
            } else {
                -box_.half_extents.z
            },
        );
        let corner = box_.center + rotation * local;
        if corner.is_finite() {
            assert!(bounds.contains(corner), "{box_:?}, {bounds:?}, {corner:?}");
        }
        contains_f64(
            bounds,
            box_.center.as_dvec3() + exact_rotation * local.as_dvec3(),
        );
    }
}

#[test]
fn sphere_simple_dimensions_and_zero_radius_are_exact() {
    let center = vec3(2.0, -3.0, 4.0);
    assert_eq!(
        Sphere {
            center,
            radius: 0.5
        }
        .bounding_aabb(),
        Some(Aabb::from_min_max(
            vec3(1.5, -3.5, 3.5),
            vec3(2.5, -2.5, 4.5)
        ))
    );
    assert_eq!(
        Sphere {
            center,
            radius: 0.0
        }
        .bounding_aabb(),
        Some(Aabb::from_min_max(center, center))
    );
    assert_eq!(Sphere::default().bounding_aabb(), Some(Aabb::default()));
    assert_eq!(
        Sphere::new(Vec3::splat(f32::MAX), -0.0).bounding_aabb(),
        Some(Aabb::from_min_max(
            Vec3::splat(f32::MAX),
            Vec3::splat(f32::MAX)
        ))
    );
}

#[test]
fn sphere_rejects_invalid_centers_and_radii() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for axis in 0..3 {
            let mut center = Vec3::ZERO;
            center[axis] = bad;
            assert!(Sphere::new(center, 1.0).bounding_aabb().is_none());
        }
        assert!(Sphere::new(Vec3::ZERO, bad).bounding_aabb().is_none());
    }
    assert!(Sphere::new(Vec3::ZERO, -f32::from_bits(1))
        .bounding_aabb()
        .is_none());
}

#[test]
fn sphere_rounds_outward_even_below_f64_precision() {
    let tiny = f32::from_bits(1);
    for center in [1.0_f32, -1.0, 1.0e30, -1.0e30] {
        let bounds = Sphere::new(Vec3::splat(center), tiny)
            .bounding_aabb()
            .unwrap();
        assert!(
            bounds.min.x < center && bounds.max.x > center,
            "{center}, {bounds:?}"
        );
        assert_eq!(bounds.min, Vec3::splat(center.next_down()));
        assert_eq!(bounds.max, Vec3::splat(center.next_up()));
    }
    let bounds = Sphere::new(Vec3::ZERO, tiny).bounding_aabb().unwrap();
    assert_eq!(bounds.min, Vec3::splat(-tiny));
    assert_eq!(bounds.max, Vec3::splat(tiny));
}

#[test]
fn sphere_detects_overflow_without_clamping_or_inflating_exact_limits() {
    assert_eq!(
        Sphere::new(Vec3::ZERO, f32::MAX).bounding_aabb(),
        Some(Aabb::from_min_max(
            Vec3::splat(-f32::MAX),
            Vec3::splat(f32::MAX)
        ))
    );
    for radius in [f32::from_bits(1), 1.0, f32::MAX] {
        for sign in [-1.0, 1.0] {
            assert!(Sphere::new(Vec3::splat(sign * f32::MAX), radius)
                .bounding_aabb()
                .is_none());
        }
    }
    // f64 addition also loses this nonzero center beside MAX.
    assert!(Sphere::new(Vec3::ONE, f32::MAX).bounding_aabb().is_none());
}

#[test]
fn sphere_bounds_enclose_surface_samples_and_axis_extrema() {
    for center in [Vec3::ZERO, vec3(1.125, -2.75, 0.375), Vec3::splat(1.0e20)] {
        for radius in [0.0, 0.1, 1.25, 1.0e15] {
            let sphere = Sphere::new(center, radius);
            let bounds = sphere.bounding_aabb().unwrap();
            finite_ordered(bounds);
            for axis in [DVec3::X, DVec3::Y, DVec3::Z] {
                contains_f64(bounds, center.as_dvec3() + axis * f64::from(radius));
                contains_f64(bounds, center.as_dvec3() - axis * f64::from(radius));
            }
            for index in 0..80 {
                let t = index as f64 * 0.37;
                let direction = DVec3::new(t.cos(), t.sin(), (t * 1.7).sin()).normalize();
                let point = center.as_dvec3() + direction * f64::from(radius);
                contains_f64(bounds, point);
                assert!(bounds.contains(point.as_vec3()));
            }
        }
    }
}

#[test]
fn capsule_uses_inner_endpoints_and_is_independent_of_endpoint_order() {
    let a = vec3(4.0, -2.0, 3.0);
    let b = vec3(-1.0, 7.0, 0.0);
    let bounds = Capsule { a, b, radius: 0.5 }.bounding_aabb().unwrap();
    assert_eq!(
        bounds,
        Aabb::from_min_max(vec3(-1.5, -2.5, -0.5), vec3(4.5, 7.5, 3.5))
    );
    assert_eq!(Capsule::new(b, a, 0.5).bounding_aabb(), Some(bounds));
    assert_eq!(
        Capsule::new(a, b, 0.0).bounding_aabb(),
        Some(Aabb::from_min_max(a.min(b), a.max(b)))
    );
    for radius in [0.0, 0.1, 10.0] {
        assert_eq!(
            Capsule::new(a, a, radius).bounding_aabb(),
            Sphere::new(a, radius).bounding_aabb()
        );
    }
}

#[test]
fn capsule_bounds_enclose_arbitrary_axis_caps_and_barrel_samples() {
    let capsule = Capsule::new(vec3(-2.0, 3.0, 7.0), vec3(5.0, -4.0, 2.0), 0.37);
    let bounds = capsule.bounding_aabb().unwrap();
    for u in [0.0, 0.125, 0.5, 0.875, 1.0] {
        let center = capsule.a.as_dvec3().lerp(capsule.b.as_dvec3(), u);
        for index in 0..100 {
            let t = index as f64 * 0.31;
            let direction = DVec3::new(t.sin(), t.cos(), (t * 2.3).sin()).normalize();
            contains_f64(bounds, center + direction * f64::from(capsule.radius));
        }
    }
}

#[test]
fn capsule_handles_extreme_segments_tiny_radii_and_overflow() {
    let a = vec3(-f32::MAX, 2.0, 0.0);
    let b = vec3(f32::MAX, -3.0, 0.0);
    assert!(!(b - a).is_finite());
    assert_eq!(
        Capsule::new(a, b, 0.0).bounding_aabb(),
        Some(Aabb::from_min_max(a.min(b), a.max(b)))
    );
    assert!(Capsule::new(a, b, f32::from_bits(1))
        .bounding_aabb()
        .is_none());
    let tiny = Capsule::new(Vec3::ONE, Vec3::splat(2.0), f32::from_bits(1))
        .bounding_aabb()
        .unwrap();
    assert_eq!(tiny.min, Vec3::splat(1.0_f32.next_down()));
    assert_eq!(tiny.max, Vec3::splat(2.0_f32.next_up()));
}

#[test]
fn capsule_rejects_invalid_endpoints_and_radius() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for axis in 0..3 {
            let mut endpoint = Vec3::ZERO;
            endpoint[axis] = bad;
            assert!(Capsule::new(endpoint, Vec3::ONE, 1.0)
                .bounding_aabb()
                .is_none());
            assert!(Capsule::new(Vec3::ONE, endpoint, 1.0)
                .bounding_aabb()
                .is_none());
        }
        assert!(Capsule::new(Vec3::ZERO, Vec3::ONE, bad)
            .bounding_aabb()
            .is_none());
    }
    assert!(Capsule::new(Vec3::ZERO, Vec3::ONE, -1.0)
        .bounding_aabb()
        .is_none());
}

#[test]
fn obb_identity_and_degenerate_extents_preserve_dimensions() {
    let center = vec3(3.0, -2.0, 1.0);
    for half_extents in [
        Vec3::ZERO,
        Vec3::X,
        vec3(2.0, 0.0, 3.0),
        vec3(0.5, 2.0, 4.0),
    ] {
        assert_eq!(
            Obb {
                center,
                half_extents,
                rotation: Quat::IDENTITY
            }
            .bounding_aabb(),
            Some(Aabb::from_min_max(
                center - half_extents,
                center + half_extents
            ))
        );
    }
    assert_eq!(Obb::default().bounding_aabb(), Some(Aabb::default()));
    check_obb_corners(Obb::new(
        center,
        vec3(0.0, 1.0, 2.0),
        Quat::from_rotation_x(0.73),
    ));
}

#[test]
fn obb_rotation_and_quaternion_scaling_match_existing_normalization() {
    let rotation = Quat::from_euler(macroquad::math::EulerRot::ZYX, 0.47, -0.88, 1.21);
    let center = vec3(2.13, -3.42, 4.71);
    let extents = vec3(0.37, 2.29, 0.81);
    for scale in [1.0, 2.0, 0.5, -1.0] {
        let box_ = Obb::new(center, extents, rotation * scale);
        check_obb_corners(box_);
        assert_eq!(
            box_.bounding_aabb(),
            Obb::new(center, extents, rotation).bounding_aabb()
        );
    }
}

#[test]
fn obb_rejects_invalid_extents_centers_and_unusable_quaternions() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for axis in 0..3 {
            let mut value = Vec3::ONE;
            value[axis] = bad;
            assert!(Obb::new(value, Vec3::ONE, Quat::IDENTITY)
                .bounding_aabb()
                .is_none());
            assert!(Obb::new(Vec3::ZERO, value, Quat::IDENTITY)
                .bounding_aabb()
                .is_none());
        }
        assert!(
            Obb::new(Vec3::ZERO, Vec3::ONE, Quat::from_xyzw(0.0, 0.0, 0.0, bad))
                .bounding_aabb()
                .is_none()
        );
    }
    for magnitude in [0.0, f32::from_bits(1), f32::MAX] {
        assert!(Obb::new(
            Vec3::ZERO,
            Vec3::ONE,
            Quat::from_xyzw(magnitude, 0.0, 0.0, 0.0)
        )
        .bounding_aabb()
        .is_none());
    }
    for axis in 0..3 {
        let mut extent = Vec3::ONE;
        extent[axis] = -f32::from_bits(1);
        assert!(Obb::new(Vec3::ZERO, extent, Quat::IDENTITY)
            .bounding_aabb()
            .is_none());
    }
}

#[test]
fn obb_tiny_extents_at_large_centers_round_outward_and_detect_overflow() {
    let tiny = Vec3::splat(f32::from_bits(1));
    let center = Vec3::splat(1.0e30);
    let bounds = Obb::new(center, tiny, Quat::IDENTITY)
        .bounding_aabb()
        .unwrap();
    assert_eq!(bounds.min, Vec3::splat(center.x.next_down()));
    assert_eq!(bounds.max, Vec3::splat(center.x.next_up()));
    for sign in [-1.0, 1.0] {
        let center = Vec3::splat(sign * f32::MAX);
        assert!(Obb::new(center, tiny, Quat::IDENTITY)
            .bounding_aabb()
            .is_none());
        assert_eq!(
            Obb::new(center, Vec3::ZERO, Quat::IDENTITY).bounding_aabb(),
            Some(Aabb::from_min_max(center, center))
        );
    }
    assert!(Obb::new(
        Vec3::ZERO,
        Vec3::splat(f32::MAX),
        Quat::from_rotation_z(0.6)
    )
    .bounding_aabb()
    .is_none());
}

#[test]
fn obb_high_precision_bounds_survive_intermediate_f32_transform_overflow() {
    let box_ = Obb::new(
        Vec3::ZERO,
        Vec3::Z * (f32::MAX * 0.9),
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
    );
    let rotation = unit_rotation(box_.rotation);
    assert!(!(rotation * box_.half_extents).is_finite());
    check_obb_corners(box_);
}

#[test]
fn deterministic_rotated_obb_samples_enclose_all_corners_without_tolerance() {
    for index in 0..384 {
        let t = index as f32 * 0.137;
        let rotation = Quat::from_euler(macroquad::math::EulerRot::XYZ, t, t * 1.7, t * -0.61);
        let center = vec3((t * 0.3).sin() * 97.0, t.cos() * 0.001, t.sin() * 13.0);
        let extents = vec3(
            (t.sin() * 7.3).abs(),
            (t.cos() * 0.17).abs(),
            0.001 + t * 0.31,
        );
        check_obb_corners(Obb::new(center, extents, rotation));
    }
}
