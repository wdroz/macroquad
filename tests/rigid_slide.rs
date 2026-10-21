use macroquad::prelude::*;

fn close(actual: Vec3, expected: Vec3) {
    assert!(
        (actual - expected).length() < 1e-3,
        "actual {actual:?}, expected {expected:?}"
    );
}

#[test]
fn hull_stops_together_at_wall_and_keeps_floor_tangent() {
    let centers = [vec3(-1.0, 0.5, 0.0), vec3(1.0, 0.5, 0.0)];
    let floor = Aabb::from_center_size(vec3(0.0, -0.5, 0.0), vec3(20.0, 1.0, 20.0));
    let wall = Aabb::from_center_size(vec3(3.0, 1.0, 0.0), vec3(1.0, 4.0, 20.0));
    let delta = vec3(4.0, -0.25, 1.0);
    let solids = [floor, wall];
    let moved = slide_spheres_solids(&centers, 0.5, delta, &solids, &[], &[], &[]);
    close(moved, vec3(1.0, 0.0, 1.0));
    let next = centers.map(|center| center + moved);
    close(next[1] - next[0], centers[1] - centers[0]);
    for center in next {
        assert!(center.x + 0.5 <= wall.min.x + 1e-4);
        assert!((center.y - 0.5).abs() < 1e-4);
    }
    let independent = centers
        .map(|center| slide_sphere_solids(center, 0.5, delta, &solids, &[], &[], &[]) - center);
    assert!(((independent[0] + independent[1]) * 0.5 - moved).length() > 0.9);
    close(
        slide_spheres_solids(
            &[centers[1], centers[0]],
            0.5,
            delta,
            &solids,
            &[],
            &[],
            &[],
        ),
        moved,
    );
}

#[test]
fn touching_floor_allows_strafe_and_separation() {
    let floor = Aabb::from_center_size(vec3(0.0, -0.5, 0.0), vec3(20.0, 1.0, 20.0));
    let centers = [vec3(-1.0, 0.5, 0.0), vec3(1.0, 0.5, 0.0)];
    for delta in [vec3(1.0, 0.0, 2.0), vec3(1.0, 0.25, 2.0)] {
        close(
            slide_spheres_solids(&centers, 0.5, delta, &[floor], &[], &[], &[]),
            delta,
        );
    }
}

#[test]
fn each_static_shape_can_stop_the_whole_group() {
    let centers = [vec3(-3.0, 0.0, 0.0), vec3(-1.0, 0.0, 0.0)];
    let boxes = [Aabb::from_center_size(Vec3::X, Vec3::ONE)];
    let obbs = [Obb::new(Vec3::X, Vec3::splat(0.5), Quat::IDENTITY)];
    let spheres = [Sphere::new(Vec3::X, 0.5)];
    let capsules = [Capsule::new(vec3(1.0, -2.0, 0.0), vec3(1.0, 2.0, 0.0), 0.5)];
    for (aabbs, obbs, spheres, capsules) in [
        (&boxes[..], &[][..], &[][..], &[][..]),
        (&[][..], &obbs[..], &[][..], &[][..]),
        (&[][..], &[][..], &spheres[..], &[][..]),
        (&[][..], &[][..], &[][..], &capsules[..]),
    ] {
        close(
            slide_spheres_solids(
                &centers,
                0.5,
                Vec3::X * 20.0,
                aabbs,
                obbs,
                spheres,
                capsules,
            ),
            Vec3::X,
        );
    }
}

#[test]
fn distinct_members_keep_the_crease_between_oblique_contact_planes() {
    let first = Obb::new(vec3(0.0, 0.0, -5.0), vec3(1.0, 5.0, 1.0), Quat::IDENTITY);
    let rotation = Quat::from_rotation_y(2.0 * std::f32::consts::PI / 3.0);
    let second = Obb::new(vec3(0.0, 0.0, 5.0), vec3(1.0, 5.0, 1.0), rotation);
    let centers = [
        first.center - Vec3::X * 1.5,
        second.center - rotation * Vec3::X * 1.5,
    ];
    let delta = vec3(1.0, 2.0, -2.0);
    for centers in [centers, [centers[1], centers[0]]] {
        let moved = slide_spheres_solids(&centers, 0.5, delta, &[], &[first, second], &[], &[]);
        close(moved, Vec3::Y * 2.0);
        for center in centers {
            for solid in [first, second] {
                assert!((center + moved - solid.closest_point(center + moved)).length() >= 0.4999);
            }
        }
    }
}

#[test]
fn a_later_obstacle_on_the_other_member_blocks_the_shared_tangent() {
    let centers = [vec3(-0.9, 1.0, 0.0), vec3(-0.9, 1.0, 3.0)];
    let rock = Sphere::new(Vec3::Y, 0.5);
    let wall = Aabb::from_center_size(vec3(0.0, 1.0, 5.0), vec3(8.0, 4.0, 0.2));
    let moved = slide_spheres_solids(
        &centers,
        0.4,
        vec3(0.5, 0.0, 8.0),
        &[wall],
        &[],
        &[rock],
        &[],
    );
    close(moved, vec3(0.0, 0.0, 1.5));
}

#[test]
fn overlapping_members_do_not_collide_with_each_other() {
    let centers = [Vec3::ZERO, Vec3::ZERO, Vec3::X * 0.1];
    let delta = vec3(1.0, 2.0, -3.0);
    close(
        slide_spheres_solids(&centers, 2.0, delta, &[], &[], &[], &[]),
        delta,
    );
}

#[test]
fn strongest_push_moves_every_member_before_the_requested_delta() {
    let floor = Aabb::from_center_size(vec3(0.0, -0.5, 0.0), vec3(20.0, 1.0, 20.0));
    let centers = [vec3(-1.0, 0.4, 0.0), vec3(1.0, 0.2, 0.0)];
    let moved = slide_spheres_solids(&centers, 0.5, vec3(0.0, -0.1, 1.0), &[floor], &[], &[], &[]);
    close(moved, vec3(0.0, 0.3, 1.0));
    close(
        (centers[1] + moved) - (centers[0] + moved),
        centers[1] - centers[0],
    );
}

#[test]
fn push_out_reuses_box_sphere_and_capsule_fallback_directions() {
    let centers = [Vec3::ZERO, Vec3::Z * 10.0];
    let aabbs = [Aabb::from_center_size(Vec3::ZERO, Vec3::ONE)];
    let obbs = [Obb::new(Vec3::ZERO, Vec3::splat(0.5), Quat::IDENTITY)];
    let spheres = [Sphere::new(Vec3::ZERO, 0.5)];
    let capsules = [Capsule::new(-Vec3::Y, Vec3::Y, 0.5)];
    for (aabbs, obbs, spheres, capsules, expected) in [
        (&aabbs[..], &[][..], &[][..], &[][..], -Vec3::X),
        (&[][..], &obbs[..], &[][..], &[][..], -Vec3::X),
        (&[][..], &[][..], &spheres[..], &[][..], Vec3::X),
        (&[][..], &[][..], &[][..], &capsules[..], Vec3::X),
    ] {
        close(
            slide_spheres_solids(&centers, 0.5, Vec3::ZERO, aabbs, obbs, spheres, capsules),
            expected,
        );
    }
}

#[test]
fn shallow_round_overlap_blocks_inward_motion_but_keeps_shared_tangent() {
    let centers = [vec3(-0.899991, 1.0, 0.0), vec3(-0.899991, 1.0, 4.0)];
    let spheres = [Sphere::new(Vec3::Y, 0.5)];
    let capsules = [Capsule::new(Vec3::ZERO, Vec3::Y * 2.0, 0.5)];
    for (spheres, capsules) in [(&spheres[..], &[][..]), (&[][..], &capsules[..])] {
        close(
            slide_spheres_solids(
                &centers,
                0.4,
                vec3(0.04, 0.0, 0.04),
                &[],
                &[],
                spheres,
                capsules,
            ),
            Vec3::Z * 0.04,
        );
    }
}

#[test]
fn single_member_matches_existing_solver() {
    let solids = [Aabb::from_center_size(Vec3::ZERO, Vec3::ONE)];
    let spheres = [Sphere::new(Vec3::X * 3.0, 0.5)];
    let capsules = [Capsule::new(-Vec3::Y, Vec3::Y, 0.5)];
    for center in [Vec3::ZERO, vec3(-2.0, 0.1, 0.2), vec3(2.0, 1.0, 3.0)] {
        for delta in [Vec3::ZERO, vec3(5.0, 0.3, -1.0), vec3(-4.0, -2.0, 1.0)] {
            let expected =
                slide_sphere_solids(center, 0.4, delta, &solids, &[], &spheres, &capsules) - center;
            assert_eq!(
                slide_spheres_solids(&[center], 0.4, delta, &solids, &[], &spheres, &capsules),
                expected
            );
        }
    }
}

#[test]
fn empty_invalid_and_zero_radius_policies_are_explicit() {
    let delta = vec3(1.0, 2.0, 3.0);
    let solids = [Aabb::from_center_size(Vec3::ZERO, Vec3::ONE)];
    assert_eq!(
        slide_spheres_solids(&[], 0.5, delta, &solids, &[], &[], &[]),
        delta
    );
    for radius in [-1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(
            slide_spheres_solids(
                &[Vec3::ZERO, Vec3::X],
                radius,
                delta,
                &solids,
                &[],
                &[],
                &[]
            ),
            delta
        );
    }
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            slide_spheres_solids(
                &[Vec3::ZERO, vec3(bad, 0.0, 0.0)],
                0.5,
                delta,
                &solids,
                &[],
                &[],
                &[]
            ),
            Vec3::ZERO
        );
        assert_eq!(
            slide_spheres_solids(&[], 0.5, vec3(bad, 0.0, 0.0), &[], &[], &[], &[]),
            Vec3::ZERO
        );
    }
    let centers = [vec3(-2.0, 0.0, 0.0), vec3(-1.0, 0.0, 0.0)];
    close(
        slide_spheres_solids(&centers, 0.0, Vec3::X * 4.0, &solids, &[], &[], &[]),
        Vec3::X * 0.5,
    );
}

#[test]
fn invalid_static_solids_are_ignored() {
    let centers = [Vec3::ZERO, Vec3::X];
    let delta = vec3(2.0, 0.0, 1.0);
    let aabbs = [Aabb::from_min_max(Vec3::ONE, -Vec3::ONE)];
    let obbs = [Obb::new(Vec3::ZERO, -Vec3::ONE, Quat::IDENTITY)];
    let spheres = [Sphere::new(Vec3::ZERO, f32::NAN)];
    let capsules = [Capsule::new(Vec3::ZERO, Vec3::Y, -1.0)];
    close(
        slide_spheres_solids(&centers, 0.5, delta, &aabbs, &obbs, &spheres, &capsules),
        delta,
    );
}

#[test]
fn tiny_tangent_survives_two_large_blocked_components() {
    let centers = [vec3(-1.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0)];
    let walls = [
        Aabb::from_center_size(vec3(5.5, 0.0, 0.0), vec3(1.0, 20.0, 20.0)),
        Aabb::from_center_size(vec3(0.0, 0.0, -5.5), vec3(20.0, 20.0, 1.0)),
    ];
    let delta = vec3(30.0, 0.0006, -26.0);
    let moved = slide_spheres_solids(&centers, 0.5, delta, &walls, &[], &[], &[]);
    assert!(
        (moved - vec3(3.5, delta.y, -4.5)).length() < 1e-6,
        "{moved:?}"
    );
}

#[test]
fn rotated_room_keeps_tangent_after_a_long_sweep() {
    // A rotated face normal has a small unit-length error. A projection must
    // not reject its own tangent because the requested move is much longer
    // than the available clearance.
    let rotation = Quat::from_xyzw(0.76431507, 0.4319868, 0.046238616, 0.47652072);
    let offset = vec3(3.0, -7.0, 11.0);
    let radius = 0.7683924;
    let centers = [
        vec3(1.506104, -0.537899, -0.62530184),
        vec3(-2.9098787, 1.5677972, -0.35892725),
        vec3(2.538797, 2.5415936, 1.42837),
        vec3(-0.9181075, 0.5550494, -2.392235),
    ]
    .map(|center| offset + rotation * center);
    let walls: [Obb; 6] = std::array::from_fn(|index| {
        let axis = index / 2;
        let mut center = Vec3::ZERO;
        center[axis] = if index % 2 == 0 { -5.5 } else { 5.5 };
        let mut half_extents = Vec3::splat(10.0);
        half_extents[axis] = 0.5;
        Obb::new(offset + rotation * center, half_extents, rotation)
    });
    let delta = rotation * vec3(24.992855, 12.559086, -29.839157);
    let moved = slide_spheres_solids(&centers, radius, delta, &[], &walls, &[], &[]);
    close(
        rotation.inverse() * moved,
        vec3(1.6928105, 1.6900139, -1.8393724),
    );
}
