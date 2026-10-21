//! Public, synthetic stair fixtures: these require no window or game assets.

use macroquad::math::{
    vec3, Aabb, Capsule, CharacterController, CharacterMotion, CharacterSolids,
    CharacterStepStatus, Obb, Quat, Sphere, Vec3,
};

fn body(center: Vec3) -> Aabb {
    Aabb::from_center_size(center, vec3(0.5, 1.0, 0.5))
}

fn floor() -> Aabb {
    Aabb::from_min_max(vec3(-20.0, -1.0, -5.0), vec3(20.0, 0.0, 5.0))
}

fn assert_clear(body: Aabb, solids: &[Aabb]) {
    for solid in solids {
        if let Some(contact) = body.contact(solid) {
            assert!(
                contact.penetration < 2e-4,
                "{body:?} inside {solid:?}: {contact:?}"
            );
        }
    }
}

#[test]
fn five_authored_risers_can_be_ascended_then_descended_without_height_drift() {
    let mut stairs = vec![floor()];
    for i in 0..5 {
        stairs.push(Aabb::from_min_max(
            vec3(0.8 + i as f32, -0.5, -2.0),
            vec3(1.8 + i as f32, 0.2 * (i + 1) as f32, 2.0),
        ));
    }
    let solids = CharacterSolids {
        aabbs: &stairs,
        ..Default::default()
    };
    let controller = CharacterController::default();
    let mut player = body(vec3(0.0, 0.5, 0.0));
    let mut ascents = 0;
    let mut descents = 0;
    for (direction, ticks) in [(1.0, 53), (-1.0, 53)] {
        for _ in 0..ticks {
            let moved = controller.move_body_with_step(
                player,
                vec3(direction, -1.0, 0.0),
                0.1,
                solids,
                0.25,
            );
            assert!(moved.motion.on_ground, "{moved:?}");
            assert!(moved.motion.ground_normal.unwrap().dot(Vec3::Y) > 0.999);
            assert!((moved.motion.body.size() - player.size()).length() < 2e-5);
            assert!(
                (moved.motion.body.min.y - player.min.y).abs() <= 0.251,
                "{moved:?}"
            );
            assert_clear(moved.motion.body, &stairs);
            ascents += usize::from(moved.step.status == CharacterStepStatus::Stepped);
            descents += usize::from(moved.step.status == CharacterStepStatus::Descended);
            player = moved.motion.body;
        }
        if direction > 0.0 {
            assert!((player.min.y - 1.0).abs() < 0.001, "{player:?}");
            assert!((player.center().x - 5.3).abs() < 0.002, "{player:?}");
        }
    }
    assert_eq!(ascents, 5);
    assert_eq!(descents, 5);
    assert!(player.min.y.abs() < 0.001, "{player:?}");
    assert!(player.center().x.abs() < 0.002, "{player:?}");
}

#[test]
fn taller_than_configured_step_stays_blocked_and_reports_obstacle() {
    let stairs = [
        floor(),
        Aabb::from_min_max(vec3(0.8, 0.0, -2.0), vec3(3.0, 0.4, 2.0)),
    ];
    let moved = CharacterController::default().move_body_with_step(
        body(vec3(0.0, 0.5, 0.0)),
        vec3(1.0, -1.0, 0.0),
        1.0,
        CharacterSolids {
            aabbs: &stairs,
            ..Default::default()
        },
        0.25,
    );
    assert!(moved.motion.body.max.x <= 0.8001, "{moved:?}");
    assert!(moved.motion.body.min.y.abs() < 0.001, "{moved:?}");
    assert!(moved.motion.on_ground);
    assert_ne!(moved.step.status, CharacterStepStatus::Stepped);
    assert!(moved.step.obstacle.is_some(), "{moved:?}");
    assert_clear(moved.motion.body, &stairs);
}

#[test]
fn a_rejected_step_keeps_the_ordinary_tangent_along_a_tall_wall() {
    let boxes = [
        floor(),
        Aabb::from_min_max(vec3(0.8, 0.0, -5.0), vec3(0.81, 3.0, 5.0)),
    ];
    let solids = CharacterSolids {
        aabbs: &boxes,
        ..Default::default()
    };
    let controller = CharacterController::default();
    let start = body(vec3(0.0, 0.5, 0.0));
    let velocity = vec3(2.0, -1.0, 1.0);
    let moved = controller.move_body_with_step(start, velocity, 1.0, solids, 0.3);
    assert_ne!(moved.step.status, CharacterStepStatus::Stepped, "{moved:?}");
    assert_eq!(
        moved.motion,
        controller.move_body(start, velocity, 1.0, solids)
    );
    assert!((moved.motion.body.center().z - 1.0).abs() < 0.001);
}

#[test]
fn a_low_ceiling_prevents_lifting_through_a_step() {
    let solids = [
        floor(),
        Aabb::from_min_max(vec3(0.8, 0.0, -2.0), vec3(3.0, 0.2, 2.0)),
        Aabb::from_min_max(vec3(-2.0, 1.1, -2.0), vec3(3.0, 2.0, 2.0)),
    ];
    let moved = CharacterController::default().move_body_with_step(
        body(vec3(0.0, 0.5, 0.0)),
        vec3(1.0, -1.0, 0.0),
        1.0,
        CharacterSolids {
            aabbs: &solids,
            ..Default::default()
        },
        0.25,
    );
    assert_eq!(
        moved.step.status,
        CharacterStepStatus::HeadBlocked,
        "{moved:?}"
    );
    assert!(moved.motion.body.max.x <= 0.8001, "{moved:?}");
    assert!(moved.motion.body.max.y <= 1.1001, "{moved:?}");
    assert!(moved.motion.on_ground);
    assert_clear(moved.motion.body, &solids);
}

#[test]
fn a_shorter_step_uses_available_headroom_below_the_configured_maximum() {
    let solids = [
        floor(),
        Aabb::from_min_max(vec3(0.8, 0.0, -2.0), vec3(3.0, 0.2, 2.0)),
        Aabb::from_min_max(vec3(-2.0, 1.25, -2.0), vec3(3.0, 2.0, 2.0)),
    ];
    let moved = CharacterController::default().move_body_with_step(
        body(vec3(0.0, 0.5, 0.0)),
        vec3(1.0, -1.0, 0.0),
        1.0,
        CharacterSolids {
            aabbs: &solids,
            ..Default::default()
        },
        0.4,
    );
    assert_eq!(moved.step.status, CharacterStepStatus::Stepped, "{moved:?}");
    assert!((moved.motion.body.min.y - 0.2).abs() < 0.001, "{moved:?}");
    assert!(
        (moved.motion.body.center().x - 1.0).abs() < 0.001,
        "{moved:?}"
    );
    assert_clear(moved.motion.body, &solids);
}

#[test]
fn unobstructed_walking_does_not_float_up_by_the_configured_step_height() {
    let floors = [floor()];
    let solids = CharacterSolids {
        aabbs: &floors,
        ..Default::default()
    };
    let controller = CharacterController::default();
    let start = body(vec3(0.0, 0.5, 0.0));
    let velocity = vec3(2.0, -1.0, 1.0);
    let ordinary = controller.move_body(start, velocity, 0.5, solids);
    let stepped = controller.move_body_with_step(start, velocity, 0.5, solids, 0.4);
    assert_eq!(stepped.motion, ordinary);
    assert_eq!(stepped.step.status, CharacterStepStatus::Unneeded);
}

#[test]
fn disabled_step_preserves_the_existing_controller_and_struct_literals() {
    let controller = CharacterController {
        up: Vec3::Y,
        max_slope_angle: 0.5,
        ground_snap_distance: 0.1,
        max_iterations: 8,
    };
    let floor_boxes = [floor()];
    let solids = CharacterSolids {
        aabbs: &floor_boxes,
        obbs: &[],
        spheres: &[],
        capsules: &[],
    };
    let start = body(vec3(0.0, 0.5, 0.0));
    let velocity = vec3(2.0, -1.0, 0.0);
    let ordinary = controller.move_body(start, velocity, 0.25, solids);
    let CharacterMotion {
        body: _,
        velocity: _,
        on_ground: _,
        ground_normal: _,
    } = ordinary;
    let disabled = controller.move_body_with_step(start, velocity, 0.25, solids, 0.0);
    assert_eq!(disabled.motion, ordinary);
    assert_eq!(disabled.step.status, CharacterStepStatus::Disabled);
}

#[test]
fn jumping_and_airborne_characters_do_not_acquire_a_step_boost() {
    let boxes = [
        floor(),
        Aabb::from_min_max(vec3(0.8, 0.0, -2.0), vec3(3.0, 0.2, 2.0)),
    ];
    let solids = CharacterSolids {
        aabbs: &boxes,
        ..Default::default()
    };
    let controller = CharacterController::default();
    for (start, velocity, status) in [
        (
            body(vec3(0.0, 0.5, 0.0)),
            vec3(1.0, 1.0, 0.0),
            CharacterStepStatus::Jumping,
        ),
        (
            body(vec3(0.0, 0.8, 0.0)),
            Vec3::X,
            CharacterStepStatus::Airborne,
        ),
    ] {
        let moved = controller.move_body_with_step(start, velocity, 0.5, solids, 0.4);
        assert_eq!(moved.step.status, status, "{moved:?}");
        assert_eq!(
            moved.motion,
            controller.move_body(start, velocity, 0.5, solids)
        );
    }
}

#[test]
fn descending_step_attachment_does_not_teleport_down_a_cliff() {
    let ledge = [Aabb::from_min_max(
        vec3(-4.0, -1.0, -2.0),
        vec3(0.0, 0.0, 2.0),
    )];
    let start = body(vec3(-0.5, 0.5, 0.0));
    let controller = CharacterController::default();
    let moved = controller.move_body_with_step(
        start,
        Vec3::X,
        1.0,
        CharacterSolids {
            aabbs: &ledge,
            ..Default::default()
        },
        0.3,
    );
    assert!(!moved.motion.on_ground, "{moved:?}");
    assert!(moved.motion.ground_normal.is_none());
    assert!(moved.motion.body.min.y.abs() < 0.001, "{moved:?}");
    assert!((moved.motion.body.center().x - 0.5).abs() < 0.001);
    assert_ne!(moved.step.status, CharacterStepStatus::Descended);
}

#[test]
fn falling_can_land_on_a_narrow_rail_but_cannot_step_up_in_midair() {
    let furniture = [
        floor(),
        Aabb::from_min_max(vec3(-1.0, 0.0, -0.06), vec3(1.0, 0.7, 0.06)),
    ];
    let solids = CharacterSolids {
        aabbs: &furniture,
        ..Default::default()
    };
    let moved = CharacterController::default().move_body_with_step(
        body(vec3(0.0, 2.0, 0.0)),
        vec3(0.0, -4.0, 0.0),
        0.5,
        solids,
        0.3,
    );
    assert!(moved.motion.on_ground, "{moved:?}");
    assert!((moved.motion.body.min.y - 0.7).abs() < 0.001, "{moved:?}");
    assert_eq!(moved.step.status, CharacterStepStatus::Airborne);
    assert_clear(moved.motion.body, &furniture);
}

#[test]
fn stepping_uses_caller_up_instead_of_world_y() {
    let solids = [
        Aabb::from_min_max(vec3(-4.0, -2.0, -1.0), vec3(4.0, 2.0, 0.0)),
        Aabb::from_min_max(vec3(0.8, -2.0, 0.0), vec3(3.0, 2.0, 0.2)),
    ];
    let controller = CharacterController {
        up: Vec3::Z * 7.0,
        ..Default::default()
    };
    let start = Aabb::from_center_size(vec3(0.0, 0.0, 0.5), vec3(0.5, 0.5, 1.0));
    let moved = controller.move_body_with_step(
        start,
        vec3(1.0, 0.0, -1.0),
        1.0,
        CharacterSolids {
            aabbs: &solids,
            ..Default::default()
        },
        0.25,
    );
    assert_eq!(moved.step.status, CharacterStepStatus::Stepped, "{moved:?}");
    assert!(moved.motion.on_ground);
    assert!(moved.motion.ground_normal.unwrap().dot(Vec3::Z) > 0.999);
    assert!((moved.step.rise - 0.2).abs() < 0.001, "{moved:?}");
    let obstacle = moved
        .step
        .obstacle
        .expect("the riser explains the step attempt");
    assert!(obstacle.normal.dot(-Vec3::X) > 0.999, "{moved:?}");
    let landing = moved
        .step
        .landing
        .expect("accepted step has a landing contact");
    assert!(landing.normal.dot(Vec3::Z) > 0.999, "{moved:?}");
    assert!((landing.point.z - 0.2).abs() < 0.001, "{moved:?}");
    assert!((moved.motion.body.min.z - 0.2).abs() < 0.001, "{moved:?}");
    assert!((moved.motion.body.center().x - 1.0).abs() < 0.001);
    assert_clear(moved.motion.body, &solids);
}

#[test]
fn inverted_gravity_steps_downward_in_world_y() {
    let boxes = [
        Aabb::from_min_max(vec3(-4.0, 0.0, -2.0), vec3(4.0, 1.0, 2.0)),
        Aabb::from_min_max(vec3(0.8, -0.2, -2.0), vec3(3.0, 0.0, 2.0)),
    ];
    let controller = CharacterController {
        up: -Vec3::Y,
        ..Default::default()
    };
    let moved = controller.move_body_with_step(
        body(vec3(0.0, -0.5, 0.0)),
        vec3(1.0, 1.0, 0.0),
        1.0,
        CharacterSolids {
            aabbs: &boxes,
            ..Default::default()
        },
        0.25,
    );
    assert_eq!(moved.step.status, CharacterStepStatus::Stepped, "{moved:?}");
    assert!(moved.motion.on_ground);
    assert!(moved.motion.ground_normal.unwrap().dot(-Vec3::Y) > 0.999);
    assert!((moved.motion.body.max.y + 0.2).abs() < 0.001, "{moved:?}");
    assert_clear(moved.motion.body, &boxes);
}

#[test]
fn step_attempt_cannot_attach_to_an_unwalkable_obb_incline() {
    let floors = [floor()];
    let ramp = [Obb::new(
        vec3(1.0, 0.4, 0.0),
        vec3(1.0, 0.05, 2.0),
        Quat::from_rotation_z(60_f32.to_radians()),
    )];
    let controller = CharacterController {
        max_slope_angle: 30_f32.to_radians(),
        ..Default::default()
    };
    let moved = controller.move_body_with_step(
        body(vec3(-0.5, 0.5, 0.0)),
        vec3(2.0, -0.1, 0.0),
        1.0,
        CharacterSolids {
            aabbs: &floors,
            obbs: &ramp,
            ..Default::default()
        },
        0.4,
    );
    assert_ne!(moved.step.status, CharacterStepStatus::Stepped, "{moved:?}");
    assert!(
        moved
            .motion
            .ground_normal
            .is_none_or(|normal| normal.dot(Vec3::Y) >= 30_f32.to_radians().cos() - 1e-5),
        "{moved:?}"
    );
    assert!(moved.motion.body.min.y < 0.01, "{moved:?}");
}

#[test]
fn descending_probe_reports_steep_support_instead_of_reaching_the_floor_below() {
    let boxes = [
        Aabb::from_min_max(vec3(-4.0, -1.0, -2.0), vec3(0.0, 0.0, 2.0)),
        Aabb::from_min_max(vec3(0.0, -1.0, -2.0), vec3(4.0, -0.65, 2.0)),
    ];
    let slope = [Obb::new(
        vec3(0.8, -2.0, 0.0),
        vec3(2.0, 0.02, 2.0),
        Quat::from_rotation_z(-70_f32.to_radians()),
    )];
    let moved = CharacterController::default().move_body_with_step(
        body(vec3(-0.5, 0.5, 0.0)),
        Vec3::X,
        1.0,
        CharacterSolids {
            aabbs: &boxes,
            obbs: &slope,
            ..Default::default()
        },
        0.7,
    );
    assert_eq!(
        moved.step.status,
        CharacterStepStatus::SteepLanding,
        "{moved:?}"
    );
    assert!(
        moved.step.landing.unwrap().normal.dot(Vec3::Y) < 0.5,
        "{moved:?}"
    );
    assert!(!moved.motion.on_ground, "{moved:?}");
    assert!(moved.motion.body.min.y.abs() < 0.001, "{moved:?}");
}

#[test]
fn descending_from_a_ramp_limits_total_drop_and_reports_it_from_initial_support() {
    let rotation = Quat::from_rotation_z(-30_f32.to_radians());
    let normal = rotation * Vec3::Y;
    let ramp = [Obb::new(
        vec3(-1.0, 0.0, 0.0),
        vec3(1.0, 0.05, 2.0),
        rotation,
    )];
    // The bottom-left corner initially rests on the ramp's top face.
    let start_height = (-0.45 - normal.x * -0.75) / normal.y;
    let start = body(vec3(-0.5, start_height + 0.5, 0.0));
    let boxes = [Aabb::from_min_max(
        vec3(0.0, start_height - 1.5, -2.0),
        vec3(4.0, start_height - 0.3, 2.0),
    )];
    let solids = CharacterSolids {
        aabbs: &boxes,
        obbs: &ramp,
        ..Default::default()
    };
    let controller = CharacterController {
        ground_snap_distance: 0.0,
        ..Default::default()
    };
    let velocity = vec3(1.2, -0.1, 0.0);
    let ordinary = controller.move_body(start, velocity, 1.0, solids);
    assert!(!ordinary.on_ground, "{ordinary:?}");
    assert!(
        (ordinary.body.min.y - start.min.y + 0.1).abs() < 0.001,
        "{ordinary:?}"
    );
    let bounded = controller.move_body_with_step(start, velocity, 1.0, solids, 0.25);
    assert_eq!(bounded.motion, ordinary, "{bounded:?}");
    assert_ne!(
        bounded.step.status,
        CharacterStepStatus::Descended,
        "{bounded:?}"
    );
    let attached = controller.move_body_with_step(start, velocity, 1.0, solids, 0.35);
    assert_eq!(
        attached.step.status,
        CharacterStepStatus::Descended,
        "{attached:?}"
    );
    assert!((attached.step.rise + 0.3).abs() < 0.001, "{attached:?}");
}

#[test]
fn low_round_solids_use_their_real_shapes_for_step_landings() {
    let floors = [floor()];
    let spheres = [Sphere::new(vec3(0.8, 0.1, 0.0), 0.1)];
    let capsules = [Capsule::new(vec3(0.8, 0.1, -0.8), vec3(0.8, 0.1, 0.8), 0.1)];
    for solids in [
        CharacterSolids {
            aabbs: &floors,
            spheres: &spheres,
            ..Default::default()
        },
        CharacterSolids {
            aabbs: &floors,
            capsules: &capsules,
            ..Default::default()
        },
    ] {
        let moved = CharacterController::default().move_body_with_step(
            body(vec3(0.0, 0.5, 0.0)),
            Vec3::X,
            0.7,
            solids,
            0.25,
        );
        assert_eq!(moved.step.status, CharacterStepStatus::Stepped, "{moved:?}");
        assert!(moved.motion.on_ground, "{moved:?}");
        assert!((moved.motion.body.min.y - 0.2).abs() < 0.001, "{moved:?}");
        assert!(
            moved.step.landing.unwrap().normal.dot(Vec3::Y) > 0.999,
            "{moved:?}"
        );
    }
}
