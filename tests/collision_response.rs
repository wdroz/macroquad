use macroquad::prelude::*;

fn ball(position: Vec3, mass: f32) -> RigidBody {
    RigidBody::new(
        position,
        RigidBodyShape::Sphere(Sphere::new(Vec3::ZERO, 1.0)),
        mass,
    )
    .unwrap()
}

fn box_body(position: Vec3, mass: f32) -> RigidBody {
    RigidBody::new(
        position,
        RigidBodyShape::Aabb(Aabb::from_min_max(-Vec3::ONE, Vec3::ONE)),
        mass,
    )
    .unwrap()
}

fn close(actual: Vec3, expected: Vec3) {
    assert!(
        (actual - expected).abs().max_element() < 2e-5,
        "{actual:?} != {expected:?}"
    );
}

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

fn rejected(
    mut left: RigidBody,
    mut right: RigidBody,
    restitution: f32,
    error: CollisionResponseError,
) {
    let before = (bits(left), bits(right));
    assert_eq!(left.resolve_collision(&mut right, restitution), Err(error));
    assert_eq!((bits(left), bits(right)), before);
}

#[test]
fn existing_literals_and_exhaustive_enums_remain_compatible() {
    let body = RigidBody {
        position: Vec3::ZERO,
        linear_velocity: Vec3::ZERO,
        mass: 1.0,
        linear_damping: 0.0,
        mode: RigidBodyMode::Dynamic,
        shape: RigidBodyShape::Sphere(Sphere {
            center: Vec3::ZERO,
            radius: 1.0,
        }),
    };
    assert_eq!(bits(body), bits(ball(Vec3::ZERO, 1.0)));
    fn old_error(error: RigidBodyError) -> u8 {
        match error {
            RigidBodyError::InvalidTime => 0,
            RigidBodyError::InvalidMass => 1,
            RigidBodyError::InvalidDamping => 2,
            RigidBodyError::InvalidPosition => 3,
            RigidBodyError::InvalidVelocity => 4,
            RigidBodyError::InvalidGravity => 5,
            RigidBodyError::InvalidShape => 6,
            RigidBodyError::NonFiniteResult => 7,
        }
    }
    assert_eq!(old_error(RigidBodyError::NonFiniteResult), 7);
}

#[test]
fn all_body_shape_pairings_return_original_contact_and_separate() {
    for left_box in [false, true] {
        for right_box in [false, true] {
            let mut left = if left_box {
                box_body(Vec3::ZERO, 1.0)
            } else {
                ball(Vec3::ZERO, 1.0)
            };
            let mut right = if right_box {
                box_body(Vec3::X * 1.5, 1.0)
            } else {
                ball(Vec3::X * 1.5, 1.0)
            };
            let shapes = (left.shape, right.shape);
            let contact = left.resolve_collision(&mut right, 0.0).unwrap().unwrap();
            assert_eq!(contact.normal, -Vec3::X);
            assert_eq!(contact.penetration, 0.5);
            assert_eq!(left.position, -Vec3::X * 0.25);
            assert_eq!(right.position, Vec3::X * 1.75);
            assert_eq!((left.shape, right.shape), shapes);
            assert_eq!(
                left.resolve_collision(&mut right, 0.0)
                    .unwrap()
                    .unwrap()
                    .penetration,
                0.0
            );
        }
    }
}

#[test]
fn inverse_mass_weights_both_separation_and_inelastic_impulse() {
    let mut left = ball(Vec3::ZERO, 1.0);
    let mut right = ball(Vec3::X, 3.0);
    left.linear_velocity = vec3(4.0, 3.0, -2.0);
    right.linear_velocity = vec3(0.0, -1.0, 7.0);
    left.resolve_collision(&mut right, 0.0).unwrap();
    assert_eq!(left.position, -Vec3::X * 0.75);
    assert_eq!(right.position, Vec3::X * 1.25);
    assert_eq!(left.linear_velocity, vec3(1.0, 3.0, -2.0));
    assert_eq!(right.linear_velocity, vec3(1.0, -1.0, 7.0));
}

#[test]
fn elastic_equal_mass_impact_exchanges_normal_velocities() {
    let mut left = ball(Vec3::ZERO, 2.0);
    let mut right = ball(Vec3::X * 2.0, 2.0);
    left.linear_velocity = vec3(3.0, 4.0, 5.0);
    right.linear_velocity = vec3(-1.0, -2.0, -3.0);
    left.resolve_collision(&mut right, 1.0).unwrap();
    assert_eq!(left.linear_velocity, vec3(-1.0, 4.0, 5.0));
    assert_eq!(right.linear_velocity, vec3(3.0, -2.0, -3.0));
    assert_eq!(left.position, Vec3::ZERO);
    assert_eq!(right.position, Vec3::X * 2.0);
}

#[test]
fn partial_restitution_has_expected_relative_speed_and_momentum() {
    let mut left = ball(Vec3::ZERO, 2.0);
    let mut right = ball(Vec3::X * 1.75, 6.0);
    left.linear_velocity = Vec3::X * 5.0;
    right.linear_velocity = -Vec3::X;
    left.resolve_collision(&mut right, 0.5).unwrap();
    assert_eq!(left.linear_velocity, -Vec3::X * 1.75);
    assert_eq!(right.linear_velocity, Vec3::X * 1.25);
    assert_eq!(
        left.linear_velocity.x * 2.0 + right.linear_velocity.x * 6.0,
        4.0
    );
}

#[test]
fn separating_overlap_only_corrects_positions() {
    let mut left = ball(Vec3::ZERO, 1.0);
    let mut right = ball(Vec3::X, 1.0);
    left.linear_velocity = vec3(-1.0, -0.0, 2.0);
    right.linear_velocity = vec3(3.0, 0.0, -2.0);
    let velocities = (
        left.linear_velocity.to_array().map(f32::to_bits),
        right.linear_velocity.to_array().map(f32::to_bits),
    );
    left.resolve_collision(&mut right, 1.0).unwrap();
    assert_eq!(
        (
            left.linear_velocity.to_array().map(f32::to_bits),
            right.linear_velocity.to_array().map(f32::to_bits)
        ),
        velocities
    );
    assert_eq!(left.position, -Vec3::X * 0.5);
    assert_eq!(right.position, Vec3::X * 1.5);
}

#[test]
fn touching_resting_and_tangent_pairs_are_exact_no_ops() {
    for velocity in [vec3(-0.0, 0.0, -0.0), Vec3::Y * 5.0] {
        let mut left = ball(Vec3::ZERO, 1.0);
        let mut right = ball(Vec3::X * 2.0, 1.0);
        left.linear_velocity = velocity;
        let before = (bits(left), bits(right));
        assert_eq!(
            left.resolve_collision(&mut right, 1.0)
                .unwrap()
                .unwrap()
                .penetration,
            0.0
        );
        assert_eq!((bits(left), bits(right)), before);
    }
}

#[test]
fn zero_mass_is_fixed_and_its_stored_velocity_is_ignored() {
    for mass in [0.0, -0.0] {
        for mode in [RigidBodyMode::Dynamic, RigidBodyMode::Kinematic] {
            let mut left = ball(Vec3::ZERO, 1.0);
            let mut right = ball(Vec3::X * 1.5, mass);
            left.linear_velocity = vec3(4.0, 2.0, 0.0);
            right.linear_velocity = -Vec3::X * 100.0;
            right.mode = mode;
            let before = bits(right);
            left.resolve_collision(&mut right, 0.5).unwrap();
            assert_eq!(left.position, -Vec3::X * 0.5);
            assert_eq!(left.linear_velocity, vec3(-2.0, 2.0, 0.0));
            assert_eq!(bits(right), before);
        }
    }
}

#[test]
fn kinematic_velocity_pushes_dynamic_without_changing_kinematic() {
    let mut left = ball(Vec3::ZERO, 1.0);
    let mut right = ball(Vec3::X * 1.5, 10.0);
    right.mode = RigidBodyMode::Kinematic;
    right.linear_velocity = -Vec3::X * 2.0;
    let before = bits(right);
    left.resolve_collision(&mut right, 0.5).unwrap();
    assert_eq!(left.position, -Vec3::X * 0.5);
    assert_eq!(left.linear_velocity, -Vec3::X * 3.0);
    assert_eq!(bits(right), before);
}

#[test]
fn two_immovable_bodies_report_contact_without_mutating() {
    for static_left in [false, true] {
        for static_right in [false, true] {
            let mut left = ball(Vec3::ZERO, if static_left { 0.0 } else { 1.0 });
            let mut right = ball(Vec3::X, if static_right { 0.0 } else { 1.0 });
            left.mode = RigidBodyMode::Kinematic;
            right.mode = RigidBodyMode::Kinematic;
            left.linear_velocity = Vec3::X;
            right.linear_velocity = -Vec3::X;
            let before = (bits(left), bits(right));
            assert!(left.resolve_collision(&mut right, 1.0).unwrap().is_some());
            assert_eq!((bits(left), bits(right)), before);
        }
    }
}

#[test]
fn separated_pairs_are_exact_no_ops() {
    let mut left = ball(vec3(-0.0, 0.0, -0.0), 1.0);
    let mut right = box_body(Vec3::Y * 4.0, 1.0);
    left.linear_velocity = Vec3::Y;
    let before = (bits(left), bits(right));
    assert_eq!(left.resolve_collision(&mut right, 1.0), Ok(None));
    assert_eq!((bits(left), bits(right)), before);
}

#[test]
fn reversed_pair_order_produces_the_same_physical_result() {
    let mut left = box_body(vec3(1.0, 0.0, 0.0), 3.0);
    let mut right = ball(vec3(2.75, 0.0, 0.0), 1.0);
    left.linear_velocity = Vec3::X * 3.0;
    right.linear_velocity = -Vec3::X;
    let (mut left_reverse, mut right_reverse) = (left, right);
    let a = left.resolve_collision(&mut right, 0.25).unwrap().unwrap();
    let b = right_reverse
        .resolve_collision(&mut left_reverse, 0.25)
        .unwrap()
        .unwrap();
    assert_eq!(left, left_reverse);
    assert_eq!(right, right_reverse);
    assert_eq!(a.normal, -b.normal);
    assert_eq!(a.point, b.point);
}

#[test]
fn offset_local_shapes_and_coincident_centers_follow_contact_conventions() {
    let mut left = ball(-Vec3::Y * 3.0, 1.0);
    left.shape = RigidBodyShape::Sphere(Sphere::new(Vec3::Y * 3.0, 1.0));
    let mut right = box_body(Vec3::Y * 2.0, 0.0);
    right.shape = RigidBodyShape::Aabb(Aabb::from_min_max(
        -Vec3::ONE - Vec3::Y * 2.0,
        Vec3::ONE - Vec3::Y * 2.0,
    ));
    let shape = left.shape;
    let contact = left.resolve_collision(&mut right, 0.0).unwrap().unwrap();
    assert_eq!(contact.normal, Vec3::X);
    assert_eq!(left.position, vec3(2.0, -3.0, 0.0));
    assert_eq!(left.shape, shape);

    let mut left = ball(Vec3::ZERO, 1.0);
    let mut right = ball(Vec3::ZERO, 1.0);
    assert_eq!(
        left.resolve_collision(&mut right, 0.0)
            .unwrap()
            .unwrap()
            .normal,
        Vec3::X
    );
    assert_eq!(left.position, Vec3::X);
    assert_eq!(right.position, -Vec3::X);
}

#[test]
fn dynamic_ball_rests_on_static_floor_after_repeated_integration() {
    let mut ball = ball(Vec3::Y, 1.0);
    let mut floor = box_body(-Vec3::Y, 0.0);
    for _ in 0..100 {
        ball.integrate(1.0 / 60.0, -Vec3::Y * 10.0).unwrap();
        ball.resolve_collision(&mut floor, 0.0).unwrap();
        close(ball.position, Vec3::Y);
        close(ball.linear_velocity, Vec3::ZERO);
    }
}

#[test]
fn subnormal_and_maximum_masses_do_not_overflow_inverse_mass_arithmetic() {
    for mass in [f32::from_bits(1), f32::MIN_POSITIVE, 1.0, f32::MAX] {
        let mut left = ball(Vec3::ZERO, mass);
        let mut right = ball(Vec3::X, mass);
        left.linear_velocity = Vec3::X;
        left.resolve_collision(&mut right, 1.0).unwrap();
        assert_eq!(left.position, -Vec3::X * 0.5);
        assert_eq!(right.position, Vec3::X * 1.5);
        assert_eq!(left.linear_velocity, Vec3::ZERO);
        assert_eq!(right.linear_velocity, Vec3::X);
    }
    let mut tiny = ball(Vec3::ZERO, f32::from_bits(1));
    let mut huge = ball(Vec3::X, f32::MAX);
    tiny.linear_velocity = Vec3::X;
    tiny.resolve_collision(&mut huge, 1.0).unwrap();
    assert_eq!(tiny.position, -Vec3::X);
    assert_eq!(tiny.linear_velocity, -Vec3::X);
    assert_eq!(huge.position, Vec3::X);
}

#[test]
fn extreme_relative_velocity_can_cancel_to_finite_results() {
    let mut left = ball(Vec3::ZERO, 1.0);
    let mut right = ball(Vec3::X * 2.0, 1.0);
    left.linear_velocity = Vec3::X * f32::MAX;
    right.linear_velocity = -Vec3::X * f32::MAX;
    left.resolve_collision(&mut right, 1.0).unwrap();
    assert_eq!(left.linear_velocity, -Vec3::X * f32::MAX);
    assert_eq!(right.linear_velocity, Vec3::X * f32::MAX);
}

#[test]
fn invalid_restitution_is_rejected_even_without_contact() {
    for restitution in [-1.0, 1.0001, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        rejected(
            ball(Vec3::ZERO, 0.0),
            ball(Vec3::X * 10.0, 0.0),
            restitution,
            CollisionResponseError::InvalidRestitution,
        );
    }
    let mut left = ball(Vec3::ZERO, 1.0);
    let mut right = ball(Vec3::X * 2.0, 1.0);
    assert!(left.resolve_collision(&mut right, -0.0).is_ok());
}

#[test]
fn all_body_inputs_are_checked_before_no_op_and_errors_are_transactional() {
    let valid = ball(Vec3::ZERO, 0.0);
    let mut invalid = valid;
    invalid.mass = -1.0;
    rejected(
        valid,
        invalid,
        0.0,
        CollisionResponseError::Body(RigidBodyError::InvalidMass),
    );
    invalid = valid;
    invalid.linear_damping = f32::NAN;
    rejected(
        invalid,
        valid,
        0.0,
        CollisionResponseError::Body(RigidBodyError::InvalidDamping),
    );
    invalid = valid;
    invalid.linear_velocity.x = f32::INFINITY;
    rejected(
        valid,
        invalid,
        0.0,
        CollisionResponseError::Body(RigidBodyError::InvalidVelocity),
    );
    invalid = valid;
    invalid.position.y = f32::NAN;
    rejected(
        invalid,
        valid,
        0.0,
        CollisionResponseError::Body(RigidBodyError::InvalidPosition),
    );
    invalid = valid;
    invalid.shape = RigidBodyShape::Sphere(Sphere::new(Vec3::ZERO, -1.0));
    rejected(
        valid,
        invalid,
        0.0,
        CollisionResponseError::Body(RigidBodyError::InvalidShape),
    );
}

#[test]
fn unrepresentable_second_body_result_does_not_commit_first_body() {
    let mut left = ball(Vec3::ZERO, 0.0);
    let mut right = ball(Vec3::X * 2.0, 1.0);
    // A moving kinematic wall can transfer more than f32::MAX speed.
    left.mass = 1.0;
    left.mode = RigidBodyMode::Kinematic;
    left.linear_velocity = Vec3::X * f32::MAX;
    right.linear_velocity = -Vec3::X * f32::MAX;
    rejected(
        left,
        right,
        1.0,
        CollisionResponseError::Body(RigidBodyError::NonFiniteResult),
    );

    // The left candidate is finite, while the right candidate overflows.
    left = ball(Vec3::ZERO, 9.0);
    left.linear_velocity = Vec3::X * f32::MAX;
    right = ball(Vec3::X, 1.0);
    right.linear_velocity = -Vec3::X * f32::MAX;
    rejected(
        left,
        right,
        1.0,
        CollisionResponseError::Body(RigidBodyError::NonFiniteResult),
    );
}

#[test]
fn tiny_and_huge_contact_arithmetic_fail_without_false_contacts_or_mutation() {
    let mut left = ball(Vec3::ZERO, 1.0);
    left.shape = RigidBodyShape::Sphere(Sphere::new(Vec3::ZERO, 1e-30));
    let mut right = left;
    right.position = Vec3::Y * 4e-30;
    rejected(left, right, 0.0, CollisionResponseError::NumericalFailure);
    right.position = Vec3::Y * 1e-30;
    rejected(left, right, 0.0, CollisionResponseError::NumericalFailure);

    left = ball(Vec3::ZERO, 1.0);
    right = ball(Vec3::X * f32::MAX, 1.0);
    rejected(left, right, 0.0, CollisionResponseError::NumericalFailure);
    left.shape = RigidBodyShape::Sphere(Sphere::new(Vec3::ZERO, f32::MAX));
    right.position = Vec3::ZERO;
    rejected(left, right, 0.0, CollisionResponseError::NumericalFailure);
}

#[test]
fn rotated_static_obb_preserves_tangent_velocity_and_returns_world_contact() {
    let rotation = Quat::from_rotation_z(0.4);
    let collider = Obb::new(vec3(3.0, -2.0, 1.0), vec3(4.0, 1.0, 4.0), rotation);
    let normal = rotation * Vec3::Y;
    let tangent = rotation * Vec3::X;
    let mut body = ball(collider.center + normal * 1.75, 1.0);
    body.linear_velocity = -normal * 4.0 + tangent * 2.0;
    let contact = body.resolve_static_obb(&collider, 0.5).unwrap().unwrap();
    close(contact.normal, normal);
    assert!((contact.penetration - 0.25).abs() < 1e-5);
    close(body.position, collider.center + normal * 2.0);
    close(body.linear_velocity, normal * 2.0 + tangent * 2.0);
}

#[test]
fn static_obb_handles_touch_separation_immovable_and_scaled_rotation() {
    let floor = Obb::new(-Vec3::Y, vec3(4.0, 1.0, 4.0), Quat::IDENTITY * 3.0);
    let mut body = ball(Vec3::Y, 1.0);
    assert_eq!(
        body.resolve_static_obb(&floor, 0.0)
            .unwrap()
            .unwrap()
            .penetration,
        0.0
    );
    body.position = Vec3::Y * 2.0;
    assert_eq!(body.resolve_static_obb(&floor, 0.0), Ok(None));
    for mass in [0.0, 1.0] {
        body = ball(Vec3::Y * 0.5, mass);
        body.mode = RigidBodyMode::Kinematic;
        body.linear_velocity = -Vec3::Y;
        let before = bits(body);
        assert!(body.resolve_static_obb(&floor, 1.0).unwrap().is_some());
        assert_eq!(bits(body), before);
    }
}

#[test]
fn invalid_or_unsupported_static_obb_calls_are_transactional() {
    let valid = Obb::new(Vec3::ZERO, Vec3::ONE, Quat::IDENTITY);
    let mut body = ball(Vec3::ZERO, 1.0);
    for collider in [
        Obb {
            center: Vec3::splat(f32::NAN),
            ..valid
        },
        Obb {
            half_extents: -Vec3::X,
            ..valid
        },
        Obb {
            rotation: Quat::from_xyzw(0.0, 0.0, 0.0, 0.0),
            ..valid
        },
        Obb {
            rotation: Quat::from_xyzw(f32::INFINITY, 0.0, 0.0, 1.0),
            ..valid
        },
    ] {
        let before = bits(body);
        assert_eq!(
            body.resolve_static_obb(&collider, 0.0),
            Err(CollisionResponseError::InvalidCollider)
        );
        assert_eq!(bits(body), before);
    }
    body = box_body(Vec3::ZERO, 1.0);
    let before = bits(body);
    assert_eq!(
        body.resolve_static_obb(&valid, 0.0),
        Err(CollisionResponseError::UnsupportedShape)
    );
    assert_eq!(bits(body), before);
    body = ball(Vec3::X * f32::MAX, 1.0);
    let collider = Obb {
        center: -Vec3::X * f32::MAX,
        ..valid
    };
    let before = bits(body);
    assert_eq!(
        body.resolve_static_obb(&collider, 0.0),
        Err(CollisionResponseError::NumericalFailure)
    );
    assert_eq!(bits(body), before);
}

#[test]
fn response_error_exposes_underlying_body_error() {
    let error = CollisionResponseError::from(RigidBodyError::InvalidMass);
    let source: &dyn std::error::Error = &error;
    assert_eq!(
        source.source().unwrap().to_string(),
        RigidBodyError::InvalidMass.to_string()
    );
    assert!(!CollisionResponseError::NumericalFailure
        .to_string()
        .is_empty());
}
