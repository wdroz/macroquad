use macroquad::prelude::*;

fn close(a: Vec3, b: Vec3) {
    assert!((a - b).length() < 1e-4, "{a:?} != {b:?}");
}

#[test]
fn two_samples_report_different_solids_and_keep_one_translation() {
    let floor = Aabb::from_center_size(vec3(0., -0.5, 0.), vec3(20., 1., 20.));
    let wall = Aabb::from_center_size(vec3(3., 1., 0.), vec3(1., 4., 20.));
    let centers = [vec3(-1., 0.5, 0.), vec3(1., 1., 0.)];
    let mut events = Vec::new();
    let delta = vec3(4., -0.25, 1.);
    let moved = slide_spheres_solids_with_contacts(
        &centers,
        0.5,
        delta,
        &[floor, wall],
        &[],
        &[],
        &[],
        |e| events.push(e),
    );
    assert_eq!(
        moved,
        slide_spheres_solids(&centers, 0.5, delta, &[floor, wall], &[], &[], &[])
    );
    close(moved, vec3(1., 0., 1.));
    assert_eq!(events.len(), 2);
    assert_eq!((events[0].sample_index, events[0].solid_index), (0, 0));
    assert_eq!((events[1].sample_index, events[1].solid_index), (1, 1));
    assert_eq!(events[0].normal, Vec3::Y);
    assert_eq!(events[1].normal, -Vec3::X);
    assert!(events
        .iter()
        .all(|e| e.phase == SphereSlidePhase::Sweep && e.solid_kind == SlideSolidKind::Aabb));
    close(
        (centers[1] + moved) - (centers[0] + moved),
        centers[1] - centers[0],
    );
}

#[test]
fn exact_ties_keep_sample_then_kind_then_index() {
    let box_ = Aabb::from_center_size(Vec3::ZERO, vec3(1., 8., 8.));
    let obb = Obb::new(Vec3::ZERO, vec3(0.5, 4., 4.), Quat::IDENTITY);
    let sphere = Sphere::new(Vec3::ZERO, 0.5);
    let capsule = Capsule::new(-Vec3::Y, Vec3::Y, 0.5);
    let mut events = Vec::new();
    slide_spheres_solids_with_contacts(
        &[vec3(-2., 0., 0.); 2],
        0.5,
        Vec3::X * 3.,
        &[box_, box_],
        &[obb],
        &[sphere],
        &[capsule],
        |e| events.push(e),
    );
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0],
        SphereSlideContact {
            solid_kind: SlideSolidKind::Aabb,
            solid_index: 0,
            sample_index: 0,
            normal: -Vec3::X,
            phase: SphereSlidePhase::Sweep
        }
    );
    events.clear();
    slide_spheres_solids_with_contacts(
        &[vec3(-2., 0., 0.)],
        0.5,
        Vec3::X * 3.,
        &[],
        &[obb, obb],
        &[sphere],
        &[capsule],
        |e| events.push(e),
    );
    assert_eq!(events[0].solid_kind, SlideSolidKind::Obb);
    events.clear();
    slide_spheres_solids_with_contacts(
        &[vec3(-2., 0., 0.)],
        0.5,
        Vec3::X * 3.,
        &[],
        &[],
        &[sphere, sphere],
        &[capsule],
        |e| events.push(e),
    );
    assert_eq!(events[0].solid_kind, SlideSolidKind::Sphere);
    assert_eq!(events[0].solid_index, 0);
    events.clear();
    slide_spheres_solids_with_contacts(
        &[vec3(-2., 0., 0.)],
        0.5,
        Vec3::X * 3.,
        &[],
        &[],
        &[],
        &[capsule, capsule],
        |e| events.push(e),
    );
    assert_eq!(events[0].solid_kind, SlideSolidKind::Capsule);
    assert_eq!(events[0].normal, -Vec3::X);
}

#[test]
fn stronger_normal_wins_before_sample_order() {
    // Both entry times are 0.5; the second sample's Y wall opposes more motion.
    let xwall = Aabb::from_center_size(vec3(2., 0., 0.), vec3(1., 2., 2.));
    let ywall = Aabb::from_center_size(vec3(10., 3., 0.), vec3(4., 1., 2.));
    let mut first = None;
    slide_spheres_solids_with_contacts(
        &[Vec3::ZERO, vec3(10., 0., 0.)],
        0.5,
        vec3(2., 4., 0.),
        &[xwall, ywall],
        &[],
        &[],
        &[],
        |e| {
            first.get_or_insert(e);
        },
    );
    let event = first.unwrap();
    assert_eq!((event.sample_index, event.solid_index), (1, 1));
    assert_eq!(event.normal, -Vec3::Y);
}

#[test]
fn initial_recovery_and_zero_motion_report_only_applied_pushes() {
    let solid = Sphere::new(Vec3::ZERO, 1.);
    for centers in [&[Vec3::ZERO][..], &[Vec3::ZERO, Vec3::ZERO][..]] {
        let mut events = Vec::new();
        let moved = slide_spheres_solids_with_contacts(
            centers,
            0.5,
            Vec3::ZERO,
            &[],
            &[],
            &[solid, solid],
            &[],
            |e| events.push(e),
        );
        close(moved, Vec3::X * 1.5);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            SphereSlideContact {
                solid_kind: SlideSolidKind::Sphere,
                solid_index: 0,
                sample_index: 0,
                normal: Vec3::X,
                phase: SphereSlidePhase::PushOut
            }
        );
        events.clear();
        slide_spheres_solids_with_contacts(centers, 0.5, -Vec3::X, &[], &[], &[solid], &[], |e| {
            events.push(e)
        });
        assert_eq!(events[0].phase, SphereSlidePhase::PushOut);
        assert_eq!(events[1].phase, SphereSlidePhase::Sweep);
    }
    let mut event = None;
    slide_spheres_solids_with_contacts(
        &[vec3(0.8, 0., 0.), Vec3::ZERO],
        0.5,
        Vec3::ZERO,
        &[],
        &[],
        &[solid],
        &[],
        |e| {
            event.get_or_insert(e);
        },
    );
    assert_eq!(event.unwrap().sample_index, 1); // Largest correction wins.
}

#[test]
fn tangency_and_caller_defined_up_have_no_ground_policy() {
    // Z is up; a tangent graze does not report a blocking sweep.
    let floor = Aabb::from_center_size(vec3(0., 0., -0.5), vec3(20., 20., 1.));
    let centers = [vec3(-1., 0., 0.5), vec3(1., 0., 0.5)];
    let mut count = 0;
    let moved =
        slide_spheres_solids_with_contacts(&centers, 0.5, Vec3::Y, &[floor], &[], &[], &[], |_| {
            count += 1
        });
    assert_eq!(count, 0);
    close(moved, Vec3::Y);
    let mut event = None;
    let moved = slide_spheres_solids_with_contacts(
        &centers,
        0.5,
        vec3(0., 1., -1.),
        &[floor],
        &[],
        &[],
        &[],
        |e| event = Some(e),
    );
    close(moved, Vec3::Y);
    assert_eq!(event.unwrap().normal, Vec3::Z);
    assert_eq!(event.unwrap().sample_index, 0);
}

#[test]
fn rotated_obb_normal_and_original_slice_index() {
    let rotation = Quat::from_rotation_z(0.4);
    let obb = Obb::new(Vec3::ZERO, Vec3::ONE, rotation);
    let normal = rotation * -Vec3::X;
    let mut event = None;
    slide_spheres_solids_with_contacts(
        &[normal * 3.],
        0.5,
        normal * -4.,
        &[],
        &[Obb::new(Vec3::ZERO, -Vec3::ONE, Quat::IDENTITY), obb],
        &[],
        &[],
        |e| event = Some(e),
    );
    let event = event.unwrap();
    assert_eq!(event.solid_index, 1);
    close(event.normal, normal);
}

#[test]
fn invalid_and_free_inputs_emit_nothing() {
    let solid = Sphere::new(Vec3::ZERO, 1.);
    for (centers, radius, delta) in [
        (vec![], 0.5, Vec3::X),
        (vec![Vec3::ZERO], -1., Vec3::X),
        (vec![Vec3::ZERO], f32::NAN, Vec3::X),
        (vec![Vec3::NAN], 0.5, Vec3::X),
        (vec![Vec3::ZERO], 0.5, Vec3::NAN),
        (vec![Vec3::X * 1.5], 0.5, Vec3::ZERO),
    ] {
        let moved = slide_spheres_solids_with_contacts(
            &centers,
            radius,
            delta,
            &[],
            &[],
            &[solid],
            &[],
            |_| panic!("unexpected contact"),
        );
        assert_eq!(
            moved,
            slide_spheres_solids(&centers, radius, delta, &[], &[], &[solid], &[])
        );
    }
}

#[test]
fn both_entry_points_agree_for_mixed_scenes() {
    let boxes = [Aabb::from_center_size(Vec3::ZERO, Vec3::ONE * 2.)];
    let obbs = [Obb::new(
        vec3(3., 1., 0.),
        Vec3::ONE,
        Quat::from_rotation_y(0.6),
    )];
    let spheres = [Sphere::new(vec3(-3., 0., 0.), 1.)];
    let capsules = [Capsule::new(vec3(0., -2., 3.), vec3(0., 2., 3.), 0.5)];
    for x in -5..=5 {
        for y in -3..=3 {
            for n in 1..=3 {
                let centers: Vec<_> = (0..n)
                    .map(|i| vec3(x as f32 + i as f32 * 0.7, y as f32, 0.2))
                    .collect();
                for delta in [Vec3::ZERO, vec3(4., -2., 3.), vec3(-5., 1., -4.)] {
                    let plain = slide_spheres_solids(
                        &centers, 0.5, delta, &boxes, &obbs, &spheres, &capsules,
                    );
                    let observed = slide_spheres_solids_with_contacts(
                        &centers,
                        0.5,
                        delta,
                        &boxes,
                        &obbs,
                        &spheres,
                        &capsules,
                        |e| {
                            assert!(e.sample_index < centers.len());
                            assert!(e.normal.is_finite());
                            assert!((e.normal.length() - 1.).abs() < 1e-4);
                        },
                    );
                    assert_eq!(plain, observed);
                }
            }
        }
    }
}
