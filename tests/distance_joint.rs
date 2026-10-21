use macroquad::prelude::*;

fn body(position: Vec3, mass: f32) -> RigidBody {
    RigidBody::new(
        position,
        RigidBodyShape::Sphere(Sphere::new(Vec3::ZERO, 0.0)),
        mass,
    )
    .unwrap()
}

fn full(rest_length: f32) -> DistanceJoint {
    DistanceJoint {
        rest_length,
        position_correction: 1.0,
        velocity_correction: 1.0,
    }
}

fn near(a: Vec3, b: Vec3) {
    assert!((a - b).length() < 1e-5, "{a:?} != {b:?}");
}

#[test]
fn defaults_and_inverse_mass_preserve_center_of_mass_and_momentum() {
    let joint = DistanceJoint::new(2.0).unwrap();
    assert_eq!(
        joint,
        DistanceJoint {
            rest_length: 2.0,
            position_correction: 0.2,
            velocity_correction: 0.2
        }
    );
    let mut a = body(Vec3::ZERO, 1.0);
    let mut b = body(Vec3::X * 6.0, 3.0);
    a.linear_velocity = vec3(-3.0, 2.0, 0.0);
    b.linear_velocity = vec3(1.0, -4.0, 0.0);
    joint.solve(&mut a, &mut b).unwrap();
    near(a.position, Vec3::X * 0.6);
    near(b.position, Vec3::X * 5.8);
    near(a.position + b.position * 3.0, Vec3::X * 18.0);
    near(
        a.linear_velocity + b.linear_velocity * 3.0,
        vec3(0.0, -10.0, 0.0),
    );
    near(b.linear_velocity - a.linear_velocity, vec3(3.2, -6.0, 0.0));
}

#[test]
fn compression_extension_and_both_radial_velocity_signs() {
    for distance in [1.0, 2.0, 6.0] {
        for speed in [-4.0, 0.0, 4.0] {
            let mut a = body(Vec3::ZERO, 1.0);
            let mut b = body(Vec3::Y * distance, 1.0);
            b.linear_velocity = vec3(7.0, speed, -3.0);
            full(2.0).solve(&mut a, &mut b).unwrap();
            near(b.position - a.position, Vec3::Y * 2.0);
            near(b.linear_velocity - a.linear_velocity, vec3(7.0, 0.0, -3.0));
        }
    }
}

#[test]
fn static_and_kinematic_anchors_in_either_argument_order() {
    for kinematic in [false, true] {
        for reversed in [false, true] {
            let mut anchor = body(Vec3::ZERO, if kinematic { 10.0 } else { 0.0 });
            if kinematic {
                anchor.mode = RigidBodyMode::Kinematic;
            }
            anchor.linear_velocity = Vec3::X * 3.0;
            let before = anchor;
            let mut bob = body(Vec3::X * 4.0, 1.0);
            bob.linear_velocity = vec3(-2.0, 5.0, 0.0);
            if reversed {
                full(2.0).solve(&mut bob, &mut anchor).unwrap();
            } else {
                full(2.0).solve(&mut anchor, &mut bob).unwrap();
            }
            assert_eq!(anchor, before);
            near(bob.position, Vec3::X * 2.0);
            near(
                bob.linear_velocity,
                vec3(if kinematic { 3.0 } else { 0.0 }, 5.0, 0.0),
            );
        }
    }
    let mut a = body(Vec3::ZERO, 0.0);
    let mut b = body(Vec3::X * 9.0, 1.0);
    b.mode = RigidBodyMode::Kinematic;
    let before = (a, b);
    full(2.0).solve(&mut a, &mut b).unwrap();
    assert_eq!((a, b), before);
}

#[test]
fn coincident_positive_length_uses_x_and_zero_length_removes_all_relative_motion() {
    let mut a = body(Vec3::ZERO, 1.0);
    let mut b = a;
    full(2.0).solve(&mut a, &mut b).unwrap();
    assert_eq!(a.position, -Vec3::X);
    assert_eq!(b.position, Vec3::X);
    for coincident in [false, true] {
        let mut a = body(vec3(1.0, 2.0, 3.0), 0.0);
        let mut b = body(
            if coincident {
                a.position
            } else {
                vec3(7.0, 9.0, -4.0)
            },
            1.0,
        );
        b.linear_velocity = vec3(3.0, -4.0, 5.0);
        full(0.0).solve(&mut a, &mut b).unwrap();
        assert_eq!(b.position, a.position);
        assert_eq!(b.linear_velocity, Vec3::ZERO);
    }
}

#[test]
fn correction_channels_are_independent_and_repeated_steps_reduce_error() {
    let mut a = body(Vec3::ZERO, 0.0);
    let mut b = body(Vec3::X * 4.0, 1.0);
    b.linear_velocity = Vec3::X * 5.0;
    DistanceJoint {
        velocity_correction: 0.0,
        ..full(2.0)
    }
    .solve(&mut a, &mut b)
    .unwrap();
    assert_eq!(b.position.x, 2.0);
    assert_eq!(b.linear_velocity.x, 5.0);
    b.position.x = 4.0;
    DistanceJoint {
        position_correction: 0.0,
        ..full(2.0)
    }
    .solve(&mut a, &mut b)
    .unwrap();
    assert_eq!(b.position.x, 4.0);
    assert_eq!(b.linear_velocity.x, 0.0);
    let joint = DistanceJoint::new(2.0).unwrap();
    let mut error = 2.0;
    for _ in 0..30 {
        joint.solve(&mut a, &mut b).unwrap();
        let next_error = b.position.x - 2.0;
        assert!(next_error >= 0.0 && next_error < error);
        error = next_error;
    }
}

#[test]
fn invalid_settings_checked_even_for_immovable_bodies() {
    let a = body(Vec3::ZERO, 0.0);
    let b = body(Vec3::X, 0.0);
    for value in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            DistanceJoint::new(value),
            Err(DistanceJointError::InvalidRestLength)
        );
        let mut pair = (a, b);
        assert_eq!(
            full(value).solve(&mut pair.0, &mut pair.1),
            Err(DistanceJointError::InvalidRestLength)
        );
        assert_eq!(pair, (a, b));
    }
    for value in [-0.1, 1.1, f32::NAN, f32::INFINITY] {
        for position in [false, true] {
            let mut joint = full(1.0);
            if position {
                joint.position_correction = value;
            } else {
                joint.velocity_correction = value;
            }
            let mut pair = (a, b);
            assert_eq!(
                joint.solve(&mut pair.0, &mut pair.1),
                Err(DistanceJointError::InvalidCorrection)
            );
            assert_eq!(pair, (a, b));
        }
    }
}

#[test]
fn invalid_body_is_transactional_even_with_disabled_corrections() {
    let a = body(Vec3::ZERO, 1.0);
    let b = body(Vec3::X, 1.0);
    for reversed in [false, true] {
        let mut pair = (a, b);
        if reversed {
            pair.0.mass = -1.0;
        } else {
            pair.1.mass = -1.0;
        }
        let before = pair;
        let joint = DistanceJoint {
            position_correction: 0.0,
            velocity_correction: 0.0,
            ..full(1.0)
        };
        assert_eq!(
            joint.solve(&mut pair.0, &mut pair.1),
            Err(DistanceJointError::Body(RigidBodyError::InvalidMass))
        );
        assert_eq!(pair, before);
    }
}

#[test]
fn unrepresentable_second_candidate_does_not_commit_first() {
    let mut a = body(Vec3::ZERO, 1.0);
    let mut b = body(Vec3::X, 1.0);
    b.shape = RigidBodyShape::Sphere(Sphere::new(Vec3::X * f32::MAX, 0.0));
    let before = (a, b);
    assert_eq!(
        full(f32::MAX).solve(&mut a, &mut b),
        Err(DistanceJointError::Body(RigidBodyError::NonFiniteResult))
    );
    assert_eq!((a, b), before);
}

#[test]
fn finite_extremes_and_subnormal_lengths_and_masses() {
    let tiny = f32::from_bits(1);
    let mut a = body(-Vec3::X * f32::MAX, tiny);
    let mut b = body(Vec3::X * f32::MAX, tiny);
    full(0.0).solve(&mut a, &mut b).unwrap();
    assert_eq!(a.position, Vec3::ZERO);
    assert_eq!(b.position, Vec3::ZERO);
    let mut a = body(Vec3::ZERO, 0.0);
    let mut b = body(Vec3::Y * tiny, f32::MAX);
    full(tiny * 2.0).solve(&mut a, &mut b).unwrap();
    assert_eq!(b.position, Vec3::Y * (tiny * 2.0));
}

#[test]
fn diagonal_joint_is_symmetric_and_preserves_local_shapes() {
    let a = body(vec3(1.0, 2.0, 3.0), 2.0);
    let b = body(vec3(4.0, 6.0, 3.0), 3.0);
    let mut forward = (a, b);
    let mut reverse = (b, a);
    full(10.0).solve(&mut forward.0, &mut forward.1).unwrap();
    full(10.0).solve(&mut reverse.0, &mut reverse.1).unwrap();
    assert_eq!(forward, (reverse.1, reverse.0));
    near(forward.1.position - forward.0.position, vec3(6.0, 8.0, 0.0));
    assert_eq!((forward.0.shape, forward.1.shape), (a.shape, b.shape));
}

#[test]
fn velocity_overflow_is_transactional() {
    let mut a = body(Vec3::ZERO, 1.0);
    let mut b = body(vec3(1.0, 1.0, 0.0), 1.0);
    b.mode = RigidBodyMode::Kinematic;
    a.linear_velocity = vec3(-f32::MAX, f32::MAX, 0.0);
    b.linear_velocity = vec3(f32::MAX, f32::MAX, 0.0);
    let before = (a, b);
    assert_eq!(
        full(1.0).solve(&mut a, &mut b),
        Err(DistanceJointError::Body(RigidBodyError::NonFiniteResult))
    );
    assert_eq!((a, b), before);
}
