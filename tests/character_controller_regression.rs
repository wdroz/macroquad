//! Public-API regressions for composed kinematic character movement.

use macroquad::math::{vec3, Aabb, CharacterController, CharacterSolids, Obb, Quat, Vec3};

fn floor() -> Aabb {
    Aabb::from_min_max(vec3(-20.0, -1.0, -20.0), vec3(20.0, 0.0, 20.0))
}

fn body(center: Vec3) -> Aabb {
    Aabb::from_center_size(center, vec3(0.5, 1.0, 0.5))
}

#[test]
fn repeated_floor_strafing_preserves_size_and_does_not_accumulate_vertical_drift() {
    let controller = CharacterController::default();
    let floors = [floor()];
    let mut player = body(vec3(-2.0, 0.5, 0.0));
    for _ in 0..240 {
        let moved = controller.move_body(
            player,
            vec3(0.5, -0.25, 0.2),
            1.0 / 60.0,
            CharacterSolids {
                aabbs: &floors,
                ..Default::default()
            },
        );
        assert!(moved.on_ground, "{moved:?}");
        assert!(moved.ground_normal.unwrap().dot(Vec3::Y) > 0.999);
        assert!((moved.body.min.y).abs() < 0.001, "{moved:?}");
        assert!((moved.body.size() - player.size()).length() < 0.00001);
        assert!(moved.velocity.y.abs() < 0.00001);
        player = moved.body;
    }
    assert!((player.center().x).abs() < 0.002, "{player:?}");
    assert!((player.center().z - 0.8).abs() < 0.002, "{player:?}");
}

#[test]
fn floor_contact_earlier_in_move_does_not_ground_a_body_that_walked_off_the_ledge() {
    let ledge = [Aabb::from_min_max(
        vec3(-2.0, -1.0, -2.0),
        vec3(0.0, 0.0, 2.0),
    )];
    let moved = CharacterController::default().move_body(
        body(vec3(-0.5, 0.5, 0.0)),
        vec3(2.0, -1.0, 0.0),
        1.0,
        CharacterSolids {
            aabbs: &ledge,
            ..Default::default()
        },
    );
    assert!(moved.body.min.x > ledge[0].max.x, "{moved:?}");
    assert!(!moved.on_ground, "{moved:?}");
    assert!(moved.ground_normal.is_none());
}

#[test]
fn floor_and_thin_wall_order_does_not_change_the_corner_slide() {
    let wall = Aabb::from_min_max(vec3(2.0, 0.0, -10.0), vec3(2.01, 4.0, 10.0));
    let mut results = Vec::new();
    for solids in [[floor(), wall], [wall, floor()]] {
        let moved = CharacterController::default().move_body(
            body(vec3(0.0, 0.5, 0.0)),
            vec3(10.0, -2.0, 1.0),
            1.0,
            CharacterSolids {
                aabbs: &solids,
                ..Default::default()
            },
        );
        assert!(moved.body.max.x <= 2.0001, "{moved:?}");
        assert!(moved.body.min.y >= -0.0001, "{moved:?}");
        assert!((moved.body.center().z - 1.0).abs() < 0.001, "{moved:?}");
        assert!(moved.on_ground, "{moved:?}");
        results.push(moved.body.center());
    }
    assert!((results[0] - results[1]).length() < 0.001, "{results:?}");
}

#[test]
fn ceiling_contact_removes_upward_velocity_without_sticking_horizontal_motion() {
    let solids = [Aabb::from_min_max(
        vec3(-5.0, 2.0, -5.0),
        vec3(5.0, 2.01, 5.0),
    )];
    let moved = CharacterController::default().move_body(
        body(vec3(0.0, 0.5, 0.0)),
        vec3(1.0, 10.0, 0.0),
        1.0,
        CharacterSolids {
            aabbs: &solids,
            ..Default::default()
        },
    );
    assert!(moved.body.max.y <= 2.0001, "{moved:?}");
    assert!((moved.body.center().x - 1.0).abs() < 0.001, "{moved:?}");
    assert!(moved.velocity.y.abs() < 0.0001, "{moved:?}");
    assert!(!moved.on_ground);
}

#[test]
fn snapping_cannot_pass_through_an_unwalkable_slope_to_a_lower_floor() {
    let controller = CharacterController {
        ground_snap_distance: 0.3,
        max_slope_angle: 45_f32.to_radians(),
        ..Default::default()
    };
    let rotation = Quat::from_rotation_z(70_f32.to_radians());
    let normal = rotation * Vec3::Y;
    let half = vec3(0.25, 0.5, 0.25);
    let start = body(normal * (0.1 + half.dot(normal.abs()) + 0.02));
    let steep = [Obb::new(Vec3::ZERO, vec3(10.0, 0.1, 10.0), rotation)];
    let lower = [Aabb::from_min_max(
        vec3(-10.0, start.min.y - 1.0, -10.0),
        vec3(10.0, start.min.y - 0.15, 10.0),
    )];
    assert!(start.sweep_obb(-Vec3::Y * 0.3, &steep[0]).is_some());
    let moved = controller.move_body(
        start,
        Vec3::ZERO,
        1.0,
        CharacterSolids {
            aabbs: &lower,
            obbs: &steep,
            ..Default::default()
        },
    );
    assert!(!moved.on_ground, "{moved:?}");
    assert!(
        (moved.body.center() - start.center()).length() < 0.0001,
        "{moved:?}"
    );
}

#[test]
fn upside_down_gravity_uses_the_supplied_up_axis_for_ground_and_jumping() {
    let controller = CharacterController {
        up: -Vec3::Y,
        ..Default::default()
    };
    let ceiling_floor = [Aabb::from_min_max(
        vec3(-5.0, 2.0, -5.0),
        vec3(5.0, 3.0, 5.0),
    )];
    let start = body(vec3(0.0, 1.5, 0.0));
    let grounded = controller.move_body(
        start,
        vec3(0.5, 1.0, 0.0),
        1.0,
        CharacterSolids {
            aabbs: &ceiling_floor,
            ..Default::default()
        },
    );
    assert!(grounded.on_ground, "{grounded:?}");
    assert!(grounded.ground_normal.unwrap().dot(-Vec3::Y) > 0.999);
    assert!((grounded.body.max.y - 2.0).abs() < 0.001);
    let jumping = controller.move_body(
        grounded.body,
        -Vec3::Y,
        0.05,
        CharacterSolids {
            aabbs: &ceiling_floor,
            ..Default::default()
        },
    );
    assert!(!jumping.on_ground, "{jumping:?}");
    assert!((jumping.body.max.y - 1.95).abs() < 0.001, "{jumping:?}");
}

#[test]
fn snapping_velocity_respects_both_the_ground_and_a_previously_touched_wall() {
    let rotation = Quat::from_rotation_z(30_f32.to_radians());
    let normal = rotation * Vec3::Y;
    let half = vec3(0.25, 0.5, 0.25);
    // A box just above a walkable slope, touching the wall on its left.
    let x = 0.25;
    let y = (0.1 + half.dot(normal.abs()) + 0.05 * normal.y - normal.x * x) / normal.y;
    let start = body(vec3(x, y, 0.0));
    let wall = [Aabb::from_min_max(
        vec3(-1.0, -10.0, -10.0),
        vec3(0.0, 10.0, 10.0),
    )];
    let ramp = [Obb::new(Vec3::ZERO, vec3(10.0, 0.1, 10.0), rotation)];
    let moved = CharacterController::default().move_body(
        start,
        vec3(-1.0, -1.0, 0.0),
        0.01,
        CharacterSolids {
            aabbs: &wall,
            obbs: &ramp,
            ..Default::default()
        },
    );
    assert!(moved.on_ground, "{moved:?}");
    let ground = moved.ground_normal.unwrap();
    assert!(moved.velocity.dot(ground) >= -0.00001, "{moved:?}");
    assert!(moved.velocity.x >= -0.00001, "{moved:?}");
}

#[test]
fn exhausted_overlap_recovery_does_not_report_a_floor_while_deeply_embedded() {
    let controller = CharacterController {
        max_iterations: 1,
        ..Default::default()
    };
    // The gap is only half as tall as this character. One downward push from
    // the ceiling embeds it deeply in the floor instead of finding support.
    let solids = [
        floor(),
        Aabb::from_min_max(vec3(-5.0, 0.5, -5.0), vec3(5.0, 2.0, 5.0)),
    ];
    let moved = controller.move_body(
        body(vec3(0.0, 0.5, 0.0)),
        Vec3::ZERO,
        1.0 / 60.0,
        CharacterSolids {
            aabbs: &solids,
            ..Default::default()
        },
    );
    assert!(!moved.on_ground, "{moved:?}");
    assert!(moved.ground_normal.is_none(), "{moved:?}");
}
