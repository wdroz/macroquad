use macroquad::prelude::*;

fn ball(mass: f32) -> RigidBody {
    RigidBody::new(
        Vec3::ZERO,
        RigidBodyShape::Sphere(Sphere::new(Vec3::ZERO, 0.5)),
        mass,
    )
    .unwrap()
}

// Compare even NaN payloads and signed zero when asserting transactional errors.
fn bits(body: RigidBody) -> Vec<u32> {
    let mut values = Vec::new();
    values.extend(body.position.to_array().map(f32::to_bits));
    values.extend(body.linear_velocity.to_array().map(f32::to_bits));
    values.extend([body.mass.to_bits(), body.linear_damping.to_bits()]);
    values.push(match body.mode {
        RigidBodyMode::Dynamic => 0,
        RigidBodyMode::Kinematic => 1,
    });
    match body.shape {
        RigidBodyShape::Aabb(bounds) => {
            values.push(0);
            values.extend(bounds.min.to_array().map(f32::to_bits));
            values.extend(bounds.max.to_array().map(f32::to_bits));
        }
        RigidBodyShape::Sphere(sphere) => {
            values.push(1);
            values.extend(sphere.center.to_array().map(f32::to_bits));
            values.push(sphere.radius.to_bits());
        }
    }
    values
}

fn rejected(mut body: RigidBody, dt: f32, gravity: Vec3, error: RigidBodyError) {
    let before = bits(body);
    assert_eq!(body.integrate(dt, gravity), Err(error));
    assert_eq!(bits(body), before);
}

#[test]
fn constructor_and_complete_literal_are_public_in_prelude() {
    let body = ball(2.0);
    assert_eq!(
        body,
        RigidBody {
            position: Vec3::ZERO,
            linear_velocity: Vec3::ZERO,
            mass: 2.0,
            linear_damping: 0.0,
            mode: RigidBodyMode::Dynamic,
            shape: RigidBodyShape::Sphere(Sphere {
                center: Vec3::ZERO,
                radius: 0.5,
            }),
        }
    );
    assert!(!body.is_static());
}

#[test]
fn dynamic_step_updates_velocity_before_position() {
    let mut body = ball(1.0);
    body.position = vec3(1.0, 4.0, 2.0);
    body.linear_velocity = vec3(2.0, 1.0, -2.0);
    body.integrate(0.5, -Vec3::Y * 8.0).unwrap();
    assert_eq!(body.linear_velocity, vec3(2.0, -3.0, -2.0));
    assert_eq!(body.position, vec3(2.0, 2.5, 1.0));
    body.integrate(0.5, -Vec3::Y * 8.0).unwrap();
    assert_eq!(body.linear_velocity, vec3(2.0, -7.0, -2.0));
    assert_eq!(body.position, vec3(3.0, -1.0, 0.0));
}

#[test]
fn gravity_does_not_depend_on_positive_mass_or_up_axis() {
    for mass in [f32::from_bits(1), 1.0, 2.0, f32::MAX] {
        let mut body = ball(mass);
        body.integrate(0.25, vec3(4.0, -8.0, 12.0)).unwrap();
        assert_eq!(body.linear_velocity, vec3(1.0, -2.0, 3.0));
        assert_eq!(body.position, vec3(0.25, -0.5, 0.75));
    }
}

#[test]
fn exponential_damping_applies_after_gravity_and_before_translation() {
    let mut body = ball(1.0);
    body.linear_velocity = vec3(2.0, -1.0, 0.0);
    body.linear_damping = 2.0;
    body.integrate(0.5, vec3(4.0, 2.0, -8.0)).unwrap();
    let expected = (4.0_f64 * (-1.0_f64).exp()) as f32;
    assert_eq!(body.linear_velocity, vec3(expected, 0.0, -expected));
    assert_eq!(body.position, body.linear_velocity * 0.5);
    assert!(body.linear_velocity.x > 0.0);
}

#[test]
fn kinematic_velocity_is_damped_but_gravity_is_ignored() {
    let mut body = ball(3.0);
    body.mode = RigidBodyMode::Kinematic;
    body.linear_velocity = Vec3::X * 4.0;
    body.linear_damping = 2.0;
    body.integrate(0.5, Vec3::splat(f32::MAX)).unwrap();
    let speed = (4.0_f64 * (-1.0_f64).exp()) as f32;
    assert_eq!(body.linear_velocity, Vec3::X * speed);
    assert_eq!(body.position, Vec3::X * (speed * 0.5));
}

#[test]
fn zero_mass_freezes_both_modes_and_preserves_velocity_bits() {
    for mass in [0.0, -0.0] {
        for mode in [RigidBodyMode::Dynamic, RigidBodyMode::Kinematic] {
            let mut body = ball(mass);
            body.mode = mode;
            body.position = vec3(2.0, -0.0, 8.0);
            body.linear_velocity = vec3(f32::MAX, -0.0, -f32::MAX);
            body.linear_damping = f32::MAX;
            let before = bits(body);
            assert!(body.is_static());
            body.integrate(f32::MAX, Vec3::splat(f32::MAX)).unwrap();
            assert_eq!(bits(body), before);
        }
    }
}

#[test]
fn zero_elapsed_time_preserves_all_bits() {
    for dt in [0.0, -0.0] {
        let mut body = ball(1.0);
        body.position = vec3(-0.0, 2.0, 3.0);
        body.linear_velocity = vec3(-0.0, f32::MAX, -f32::MAX);
        let before = bits(body);
        body.integrate(dt, Vec3::splat(f32::MAX)).unwrap();
        assert_eq!(bits(body), before);
    }
}

#[test]
fn invalid_elapsed_time_is_transactional_including_static_bodies() {
    for dt in [
        -1.0,
        -f32::from_bits(1),
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ] {
        for mass in [0.0, 1.0] {
            rejected(ball(mass), dt, Vec3::ZERO, RigidBodyError::InvalidTime);
        }
    }
}

#[test]
fn invalid_mass_is_rejected_at_construction_and_after_public_edits() {
    for mass in [
        -1.0,
        -f32::from_bits(1),
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ] {
        let mut body = ball(1.0);
        assert_eq!(
            RigidBody::new(body.position, body.shape, mass),
            Err(RigidBodyError::InvalidMass)
        );
        body.mass = mass;
        rejected(body, 0.0, Vec3::ZERO, RigidBodyError::InvalidMass);
        rejected(body, 1.0, Vec3::ZERO, RigidBodyError::InvalidMass);
    }
}

#[test]
fn invalid_damping_is_rejected_without_mutation() {
    for damping in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for mass in [0.0, 1.0] {
            let mut body = ball(mass);
            body.linear_damping = damping;
            rejected(body, 1.0, Vec3::ZERO, RigidBodyError::InvalidDamping);
        }
    }
}

#[test]
fn nonfinite_vectors_are_rejected_on_every_axis() {
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for axis in 0..3 {
            let mut vector = Vec3::ZERO;
            vector[axis] = invalid;
            let mut body = ball(1.0);
            body.position = vector;
            rejected(body, 1.0, Vec3::ZERO, RigidBodyError::InvalidPosition);
            let mut body = ball(1.0);
            body.linear_velocity = vector;
            rejected(body, 1.0, Vec3::ZERO, RigidBodyError::InvalidVelocity);
            // All calls validate gravity, even where it would not be consumed.
            for mass in [0.0, 1.0] {
                for mode in [RigidBodyMode::Dynamic, RigidBodyMode::Kinematic] {
                    for dt in [0.0, 1.0] {
                        let mut body = ball(mass);
                        body.mode = mode;
                        rejected(body, dt, vector, RigidBodyError::InvalidGravity);
                    }
                }
            }
        }
    }
}

#[test]
fn invalid_shapes_are_checked_even_for_zero_time_or_static_bodies() {
    let invalid_shapes = [
        RigidBodyShape::Sphere(Sphere::new(Vec3::ZERO, -1.0)),
        RigidBodyShape::Sphere(Sphere::new(Vec3::ZERO, f32::NAN)),
        RigidBodyShape::Sphere(Sphere::new(Vec3::ZERO, f32::INFINITY)),
        RigidBodyShape::Sphere(Sphere::new(Vec3::splat(f32::NAN), 1.0)),
        RigidBodyShape::Aabb(Aabb::from_min_max(Vec3::ONE, Vec3::ZERO)),
        RigidBodyShape::Aabb(Aabb::from_min_max(Vec3::ZERO, Vec3::splat(f32::INFINITY))),
        RigidBodyShape::Aabb(Aabb::from_min_max(Vec3::splat(f32::NAN), Vec3::ONE)),
    ];
    for shape in invalid_shapes {
        for mass in [0.0, 1.0] {
            assert_eq!(
                RigidBody::new(Vec3::ZERO, shape, mass),
                Err(RigidBodyError::InvalidShape)
            );
            let mut body = ball(mass);
            body.shape = shape;
            rejected(body, 0.0, Vec3::ZERO, RigidBodyError::InvalidShape);
        }
    }
}

#[test]
fn degenerate_shapes_and_negative_zero_radius_are_valid() {
    for shape in [
        RigidBodyShape::Aabb(Aabb::from_min_max(Vec3::ONE, Vec3::ONE)),
        RigidBodyShape::Aabb(Aabb::from_min_max(Vec3::ZERO, Vec3::X)),
        RigidBodyShape::Sphere(Sphere::new(Vec3::ZERO, -0.0)),
    ] {
        let mut body = RigidBody::new(Vec3::ZERO, shape, 1.0).unwrap();
        body.integrate(1.0, Vec3::X).unwrap();
        assert_eq!(body.position, Vec3::X);
        assert_eq!(body.shape, shape);
    }
}

#[test]
fn world_shapes_translate_offsets_and_preserve_local_geometry() {
    let position = vec3(5.0, -3.0, 9.0);
    let bounds = Aabb::from_min_max(vec3(-2.0, 1.0, 0.0), vec3(4.0, 2.0, 3.0));
    let mut body = RigidBody::new(position, RigidBodyShape::Aabb(bounds), 1.0).unwrap();
    assert_eq!(
        body.world_shape().unwrap(),
        RigidBodyShape::Aabb(Aabb::from_min_max(
            vec3(3.0, -2.0, 9.0),
            vec3(9.0, -1.0, 12.0),
        ))
    );
    body.integrate(1.0, Vec3::Y).unwrap();
    assert_eq!(body.shape, RigidBodyShape::Aabb(bounds));
    let sphere = Sphere::new(vec3(2.0, -1.0, 4.0), 3.0);
    body.shape = RigidBodyShape::Sphere(sphere);
    assert_eq!(
        body.world_shape().unwrap(),
        RigidBodyShape::Sphere(Sphere::new(vec3(7.0, -3.0, 13.0), 3.0,))
    );
    // This geometry query does not depend on integration-only properties.
    body.mass = f32::NAN;
    body.linear_damping = -1.0;
    body.linear_velocity = Vec3::splat(f32::INFINITY);
    assert!(body.world_shape().is_ok());
}

#[test]
fn invalid_world_coordinates_are_rejected_before_and_after_motion() {
    for shape in [
        RigidBodyShape::Aabb(Aabb::from_min_max(
            Vec3::splat(f32::MAX),
            Vec3::splat(f32::MAX),
        )),
        RigidBodyShape::Sphere(Sphere::new(Vec3::splat(f32::MAX), 0.0)),
    ] {
        assert_eq!(
            RigidBody::new(Vec3::splat(f32::MAX), shape, 1.0),
            Err(RigidBodyError::NonFiniteResult)
        );
        let mut body = RigidBody::new(Vec3::ZERO, shape, 1.0).unwrap();
        body.linear_velocity = Vec3::splat(f32::MAX);
        rejected(body, 1.0, Vec3::ZERO, RigidBodyError::NonFiniteResult);
    }
}

#[test]
fn nonrepresentable_updated_velocity_or_position_is_transactional() {
    let mut body = ball(1.0);
    body.linear_velocity = Vec3::X * f32::MAX;
    rejected(
        body,
        1.0,
        Vec3::X * f32::MAX,
        RigidBodyError::NonFiniteResult,
    );
    body.position = Vec3::X * f32::MAX;
    rejected(body, 1.0, Vec3::ZERO, RigidBodyError::NonFiniteResult);
}

#[test]
fn wide_intermediates_avoid_overflow_that_damping_or_cancellation_resolves() {
    let mut body = ball(1.0);
    body.linear_velocity = Vec3::X * f32::MAX;
    body.linear_damping = 1.0;
    body.integrate(1.0, Vec3::X * f32::MAX).unwrap();
    assert!(body.linear_velocity.is_finite());
    assert!(body.linear_velocity.x > f32::MAX * 0.5);
    assert_eq!(body.position, body.linear_velocity);

    let mut body = ball(1.0);
    body.position = -Vec3::X * f32::MAX;
    body.linear_velocity = Vec3::X * f32::MAX;
    body.integrate(2.0, Vec3::ZERO).unwrap();
    assert_eq!(body.position, Vec3::X * f32::MAX);
}

#[test]
fn extreme_damping_stops_motion_without_nan() {
    let mut body = ball(1.0);
    body.linear_velocity = Vec3::splat(f32::MAX);
    body.linear_damping = f32::MAX;
    body.integrate(f32::MAX, Vec3::splat(f32::MAX)).unwrap();
    assert_eq!(body.linear_velocity, Vec3::ZERO);
    assert_eq!(body.position, Vec3::ZERO);
}

#[test]
fn cloned_bodies_and_mode_changes_remain_caller_owned() {
    let mut dynamic = ball(1.0);
    let mut kinematic = dynamic;
    kinematic.mode = RigidBodyMode::Kinematic;
    kinematic.linear_velocity = Vec3::X;
    dynamic.integrate(1.0, -Vec3::Z).unwrap();
    kinematic.integrate(1.0, -Vec3::Z).unwrap();
    assert_eq!(dynamic.position, -Vec3::Z);
    assert_eq!(kinematic.position, Vec3::X);
    kinematic.mass = 0.0;
    kinematic.integrate(1.0, -Vec3::Z).unwrap();
    assert_eq!(kinematic.position, Vec3::X);
    kinematic.mass = 1.0;
    kinematic.integrate(1.0, -Vec3::Z).unwrap();
    assert_eq!(kinematic.position, Vec3::X * 2.0);
}

#[test]
fn positive_subnormal_time_is_valid_and_error_has_a_message() {
    let mut body = ball(1.0);
    body.linear_velocity = Vec3::ONE;
    let dt = f32::from_bits(1);
    body.integrate(dt, Vec3::ZERO).unwrap();
    assert_eq!(body.position, Vec3::splat(dt));
    assert!(!RigidBodyError::InvalidTime.to_string().is_empty());
    let error: &dyn std::error::Error = &RigidBodyError::InvalidMass;
    assert!(error.source().is_none());
}
