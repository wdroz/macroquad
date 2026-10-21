//! Public-API regression tests for the full translating box body.

use macroquad::math::{vec3, Aabb, Obb, Quat, Sphere, SweepHit, Vec3};

fn translated(body: Aabb, delta: Vec3) -> Aabb {
    Aabb::from_min_max(body.min + delta, body.max + delta)
}

fn assert_vec(actual: Vec3, expected: Vec3, tolerance: f32) {
    assert!(
        (actual - expected).length() <= tolerance,
        "actual {actual:?}, expected {expected:?}"
    );
}

fn assert_contact_geometry(body: Aabb, delta: Vec3, solid: Obb, hit: SweepHit) {
    assert!(hit.t.is_finite() && (0.0..=1.0).contains(&hit.t));
    assert!((hit.normal.length() - 1.0).abs() < 2e-5);
    assert!(hit.point.is_finite());
    let impact = translated(body, delta * hit.t);
    let tolerance = 3e-4;
    assert!(impact.expanded(tolerance).contains(hit.point), "{hit:?}");
    let local = solid.rotation.normalize().inverse() * (hit.point - solid.center);
    assert!(
        local
            .abs()
            .cmple(solid.half_extents + Vec3::splat(tolerance))
            .all(),
        "point outside static solid: hit {hit:?}, local {local:?}, solid {solid:?}"
    );
}

#[test]
fn thin_tilted_wall_uses_the_body_projected_extent() {
    let rotation = Quat::from_rotation_z(0.37) * Quat::from_rotation_y(0.61);
    let normal = rotation * Vec3::X;
    let solid = Obb::new(vec3(3.0, -2.0, 5.0), vec3(0.015, 4.0, 4.0), rotation);
    let half = vec3(0.2, 0.95, 0.3);
    let body = Aabb::from_center_size(solid.center - normal * 5.0, half * 2.0);
    let delta = normal * 10.0;
    assert!(!solid.overlaps_aabb(&body));
    assert!(!solid.overlaps_aabb(&translated(body, delta)));

    let hit = body.sweep_obb(delta, &solid).unwrap();
    let expected = (5.0 - solid.half_extents.x - normal.abs().dot(half)) / 10.0;
    assert!((hit.t - expected).abs() < 2e-6, "{hit:?}, {expected}");
    assert_vec(hit.normal, -normal, 2e-6);
    assert!(((hit.point - solid.center).dot(normal) + 0.015).abs() < 2e-5);
    assert_contact_geometry(body, delta, solid, hit);
}

#[test]
fn broad_thin_faces_do_not_inflate_their_own_axis_projection() {
    let rotation = Quat::from_rotation_z(0.37) * Quat::from_rotation_y(0.61);
    for thin_axis in 0..3 {
        let normal = rotation * [Vec3::X, Vec3::Y, Vec3::Z][thin_axis];
        for broad_extent in [1e4, 1e10, 1e20, 1e30] {
            let mut half_extents = Vec3::splat(broad_extent);
            half_extents[thin_axis] = 0.01;
            let solid = Obb::new(Vec3::ZERO, half_extents, rotation);
            let body = Aabb::from_center_size(-normal * 5.0, Vec3::splat(0.4));
            let delta = normal * 10.0;
            let hit = body.sweep_obb(delta, &solid).unwrap_or_else(|| {
                panic!("missed broad face: axis {thin_axis}, extent {broad_extent}")
            });
            let expected = (5.0 - 0.01 - normal.abs().dot(Vec3::splat(0.2))) / 10.0;
            assert!((hit.t - expected).abs() < 2e-6, "{hit:?}, {expected}");
            assert_vec(hit.normal, -normal, 2e-6);
            assert_contact_geometry(body, delta, solid, hit);
            assert!(body.sweep_obb(Vec3::ZERO, &solid).is_none());
            assert!(body.sweep_obb(-normal, &solid).is_none());
        }
    }
}

#[test]
fn long_thin_crossing_edges_keep_their_shared_contact_point() {
    let rotation = Quat::from_rotation_z(0.37) * Quat::from_rotation_y(0.61);
    let axes = [Vec3::X, Vec3::Y, Vec3::Z];
    for world_axis in 0..3 {
        for box_axis in 0..3 {
            let normal = axes[world_axis]
                .cross(rotation * axes[box_axis])
                .normalize();
            for long_extent in [1e4, 1e12, 1e30] {
                let mut solid_half = Vec3::splat(0.01);
                solid_half[box_axis] = long_extent;
                let solid = Obb::new(Vec3::ZERO, solid_half, rotation);
                let mut body_half = Vec3::splat(0.01);
                body_half[world_axis] = long_extent;
                let body = Aabb::from_center_size(-normal * 5.0, body_half * 2.0);
                let delta = normal * 10.0;
                let hit = body.sweep_obb(delta, &solid).unwrap_or_else(|| {
                    panic!("missed edges: world {world_axis}, box {box_axis}, extent {long_extent}")
                });
                let mut radius = 0.0;
                for axis in 0..3 {
                    if axis != world_axis {
                        radius += 0.01 * normal[axis].abs();
                    }
                    if axis != box_axis {
                        radius += 0.01 * normal.dot(rotation * axes[axis]).abs();
                    }
                }
                let expected = (5.0 - radius) / 10.0;
                assert!((hit.t - expected).abs() < 2e-6, "{hit:?}, {expected}");
                assert_vec(hit.normal, -normal, 2e-6);
                assert_contact_geometry(body, delta, solid, hit);
                assert!(body.sweep_obb(axes[world_axis] * 10.0, &solid).is_none());
            }
        }
    }
}

#[test]
fn tall_body_hits_overhang_missed_by_center_ray_and_small_sphere() {
    let solid = Obb::new(
        vec3(0.0, 1.3, 0.0),
        vec3(0.025, 0.1, 1.0),
        Quat::from_rotation_y(0.72),
    );
    let body = Aabb::from_center_size(vec3(-3.0, 0.0, 0.0), vec3(0.3, 3.0, 0.3));
    let delta = vec3(6.0, 0.0, 0.0);
    assert!(solid.ray_intersect(body.center(), delta).is_none());
    assert!(Sphere::new(body.center(), 0.15)
        .sweep_obb(delta, &solid)
        .is_none());
    let hit = body.sweep_obb(delta, &solid).unwrap();
    assert!(hit.t > 0.0 && hit.t < 0.5);
    assert!(hit.normal.dot(delta) < 0.0);
    assert_contact_geometry(body, delta, solid, hit);
}

#[test]
fn rotated_wall_corner_graze_hits_while_the_center_ray_misses() {
    let rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4);
    let solid = Obb::new(Vec3::ZERO, vec3(0.05, 1.0, 1.0), rotation);
    let z_reach = (rotation * Vec3::X).z.abs() * solid.half_extents.x
        + (rotation * Vec3::Z).z.abs() * solid.half_extents.z;
    let body = Aabb::from_center_size(vec3(-2.0, 0.0, z_reach + 0.1), Vec3::splat(0.4));
    let delta = vec3(4.0, 0.0, 0.0);
    assert!(solid.ray_intersect(body.center(), delta).is_none());
    assert!(!solid.overlaps_aabb(&body));
    assert!(!solid.overlaps_aabb(&translated(body, delta)));
    let hit = body.sweep_obb(delta, &solid).unwrap();
    assert_contact_geometry(body, delta, solid, hit);
    assert!(!solid.overlaps_aabb(&translated(body, delta * (hit.t - 0.001))));
    assert!(solid.overlaps_aabb(&translated(body, delta * (hit.t + 0.001))));
}

#[test]
fn resting_on_a_rotated_face_allows_tangent_and_away_motion() {
    // An arbitrary face normal exercises every world component; there is no up-axis assumption.
    let rotation = Quat::from_rotation_x(0.32) * Quat::from_rotation_z(0.47);
    let outward = rotation * Vec3::Y;
    let half = vec3(0.2, 0.7, 0.3);
    let solid = Obb::new(vec3(-1.0, 3.0, 2.0), vec3(5.0, 0.2, 5.0), rotation);
    let center = solid.center + outward * (solid.half_extents.y + outward.abs().dot(half));
    let body = Aabb::from_center_size(center, half * 2.0);
    assert!(body.sweep_obb(rotation * Vec3::X, &solid).is_none());
    assert!(body.sweep_obb(outward, &solid).is_none());
    assert!(body.sweep_obb(Vec3::ZERO, &solid).is_none());
    let hit = body.sweep_obb(-outward, &solid).unwrap();
    assert_eq!(hit.t, 0.0);
    assert_vec(hit.normal, outward, 2e-6);
    assert_contact_geometry(body, -outward, solid, hit);
}

#[test]
fn initial_overlap_is_reported_when_stationary_or_leaving() {
    let solid = Obb::new(
        vec3(1.0, -3.0, 2.0),
        vec3(2.0, 1.0, 1.0),
        Quat::from_rotation_y(0.83),
    );
    let body = Aabb::from_center_size(solid.center + vec3(0.2, 0.3, 0.4), vec3(0.3, 0.6, 0.4));
    for delta in [Vec3::ZERO, vec3(20.0, -10.0, 30.0)] {
        let hit = body.sweep_obb(delta, &solid).unwrap();
        assert_eq!(hit.t, 0.0);
        assert_contact_geometry(body, delta, solid, hit);
    }
}

#[test]
fn contained_body_normal_uses_escape_distance_instead_of_intersection_width() {
    let rotation = Quat::from_rotation_z(0.38) * Quat::from_rotation_y(0.49);
    let outward = rotation * Vec3::X;
    let solid = Obb::new(vec3(2.0, -1.0, 3.0), vec3(1.0, 3.0, 4.0), rotation);
    let body = Aabb::from_center_size(solid.center + outward * 0.5, vec3(0.1, 0.16, 0.12));
    let hit = body.sweep_obb(Vec3::ZERO, &solid).unwrap();
    assert_eq!(hit.t, 0.0);
    assert_vec(hit.normal, outward, 2e-6);
    assert_contact_geometry(body, Vec3::ZERO, solid, hit);
}

#[test]
fn initial_corner_touch_matches_existing_any_into_face_convention() {
    let other = Aabb::from_min_max(Vec3::ZERO, Vec3::ONE);
    let solid = Obb::new(other.center(), other.half_extents(), Quat::IDENTITY);
    let body = Aabb::from_min_max(vec3(-1.0, 1.0, 0.2), vec3(0.0, 2.0, 0.8));
    for delta in [
        Vec3::Z,
        vec3(-1.0, 1.0, 0.0),
        vec3(1.0, 1.0, 0.0),
        vec3(1.0, -1.0, 0.0),
    ] {
        match (body.sweep(delta, &other), body.sweep_obb(delta, &solid)) {
            (None, None) => {}
            (Some(expected), Some(actual)) => {
                assert_eq!(actual.t, 0.0);
                assert_vec(actual.normal, expected.normal, 2e-6);
                assert_contact_geometry(body, delta, solid, actual);
            }
            result => panic!("corner conventions disagree: {result:?}, {delta:?}"),
        }
    }
}

#[test]
fn identity_rotation_agrees_with_existing_aabb_query() {
    let other = Aabb::from_center_size(vec3(2.0, 1.0, -1.0), vec3(1.0, 2.0, 3.0));
    let solid = Obb::new(other.center(), other.half_extents(), Quat::IDENTITY);
    let cases = [
        (
            vec3(-1.0, 1.0, -1.0),
            vec3(0.4, 1.5, 0.6),
            vec3(5.0, 0.0, 0.0),
        ),
        (vec3(2.0, 4.0, -1.0), Vec3::ONE, vec3(0.0, -5.0, 0.0)),
        (vec3(2.0, 1.0, -5.0), Vec3::ONE, vec3(0.0, 0.0, 8.0)),
        (vec3(2.0, 2.5, -1.0), Vec3::ONE, vec3(2.0, 0.0, 0.0)),
        (vec3(2.0, 2.5, -1.0), Vec3::ONE, vec3(0.0, -1.0, 0.0)),
        (vec3(2.0, 1.0, -1.0), vec3(0.3, 0.4, 0.5), Vec3::ZERO),
        (vec3(-1.0, 5.0, -1.0), Vec3::ONE, vec3(5.0, 0.0, 0.0)),
    ];
    for (center, size, delta) in cases {
        let body = Aabb::from_center_size(center, size);
        match (body.sweep(delta, &other), body.sweep_obb(delta, &solid)) {
            (None, None) => {}
            (Some(expected), Some(actual)) => {
                assert!((actual.t - expected.t).abs() < 2e-6);
                assert_vec(actual.normal, expected.normal, 2e-6);
                assert_vec(actual.point, expected.point, 2e-5);
            }
            result => panic!("identity query disagrees: {result:?}, {body:?}, {delta:?}"),
        }
    }
}

#[test]
fn flat_solid_and_point_body_have_finite_continuous_hits() {
    let rotation = Quat::from_rotation_y(0.41) * Quat::from_rotation_x(0.33);
    let normal = rotation * Vec3::Z;
    let solid = Obb::new(Vec3::ZERO, vec3(2.0, 2.0, 0.0), rotation);
    let body = Aabb::from_center_size(-normal * 2.0, Vec3::ZERO);
    let delta = normal * 4.0;
    let hit = body.sweep_obb(delta, &solid).unwrap();
    assert!((hit.t - 0.5).abs() < 2e-6);
    assert_vec(hit.normal, -normal, 2e-6);
    assert_contact_geometry(body, delta, solid, hit);
}

#[test]
fn quaternion_scale_does_not_change_the_solid() {
    let rotation = Quat::from_rotation_x(0.21) * Quat::from_rotation_y(0.76);
    let solid = Obb::new(vec3(1.0, -2.0, 3.0), vec3(0.2, 2.0, 1.0), rotation);
    let body = Aabb::from_center_size(vec3(-3.0, -2.0, 3.0), vec3(0.3, 1.4, 0.4));
    let delta = vec3(8.0, 0.0, 0.0);
    let expected = body.sweep_obb(delta, &solid).unwrap();
    for scale in [0.01, 7.0, -2.0] {
        let scaled = Obb {
            rotation: rotation * scale,
            ..solid
        };
        let hit = body.sweep_obb(delta, &scaled).unwrap();
        assert!((hit.t - expected.t).abs() < 2e-6);
        assert_vec(hit.normal, expected.normal, 2e-6);
        assert_vec(hit.point, expected.point, 2e-5);
    }
}

#[test]
fn large_finite_corners_do_not_overflow_center_or_contact_arithmetic() {
    let body = Aabb::from_min_max(vec3(1.8e38, -0.5, -0.5), vec3(1.9e38, 0.5, 0.5));
    assert!(
        !body.center().is_finite(),
        "fixture must exceed f32 center arithmetic"
    );
    let solid = Obb::new(
        vec3(2.4e38, 0.0, 0.0),
        vec3(1.0e37, 2.0, 2.0),
        Quat::IDENTITY,
    );
    let delta = vec3(1.0e38, 0.0, 0.0);
    let expected = ((solid.center.x as f64 - solid.half_extents.x as f64) - body.max.x as f64)
        / delta.x as f64;
    let hit = body.sweep_obb(delta, &solid).unwrap();
    assert!((hit.t as f64 - expected).abs() < 3e-8);
    assert_eq!(hit.normal, -Vec3::X);
    assert!(hit.point.is_finite());
    let expected_point_x = (solid.center.x as f64 - solid.half_extents.x as f64) as f32;
    assert!((hit.point.x / expected_point_x - 1.0).abs() < 2e-7);
    assert!(hit.point.y.abs() <= 0.5 && hit.point.z.abs() <= 0.5);
}

#[test]
fn nearly_parallel_axes_keep_a_valid_hit_and_shared_point() {
    for angle in [1e-7, 1e-5, 1e-3] {
        let rotation = Quat::from_rotation_z(angle) * Quat::from_rotation_y(angle * 0.7);
        let solid = Obb::new(vec3(0.3, -0.2, 0.1), vec3(0.05, 1.0, 1.0), rotation);
        let body = Aabb::from_center_size(vec3(-3.0, 0.2, 0.3), vec3(0.4, 1.5, 0.6));
        let delta = vec3(6.0, 0.0, 0.0);
        let hit = body.sweep_obb(delta, &solid).unwrap();
        assert_contact_geometry(body, delta, solid, hit);
        assert!(hit.normal.x < -0.999);
        assert!(!solid.overlaps_aabb(&translated(body, delta * (hit.t - 1e-4))));
        assert!(solid.overlaps_aabb(&translated(body, delta * (hit.t + 1e-4))));
    }
}

#[test]
fn unusable_inputs_miss() {
    let solid = Obb::new(Vec3::ZERO, Vec3::ONE, Quat::IDENTITY);
    let body = Aabb::from_center_size(vec3(-3.0, 0.0, 0.0), Vec3::ONE);
    let delta = vec3(6.0, 0.0, 0.0);
    for bad in [
        Obb {
            center: vec3(f32::NAN, 0.0, 0.0),
            ..solid
        },
        Obb {
            half_extents: vec3(1.0, -1.0, 1.0),
            ..solid
        },
        Obb {
            half_extents: vec3(1.0, f32::INFINITY, 1.0),
            ..solid
        },
        Obb {
            rotation: Quat::from_xyzw(0.0, 0.0, 0.0, 0.0),
            ..solid
        },
        Obb {
            rotation: Quat::from_xyzw(f32::NAN, 0.0, 0.0, 1.0),
            ..solid
        },
    ] {
        assert!(body.sweep_obb(delta, &bad).is_none());
    }
    assert!(Aabb::from_min_max(Vec3::ONE, Vec3::ZERO)
        .sweep_obb(delta, &solid)
        .is_none());
    assert!(
        Aabb::from_min_max(Vec3::splat(f32::NEG_INFINITY), Vec3::ZERO)
            .sweep_obb(delta, &solid)
            .is_none()
    );
    assert!(body.sweep_obb(vec3(f32::NAN, 0.0, 0.0), &solid).is_none());
}

struct Samples(u32);

impl Samples {
    fn scalar(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / 16_777_216.0
    }

    fn vector(&mut self) -> Vec3 {
        vec3(self.scalar(), self.scalar(), self.scalar()) * 2.0 - Vec3::ONE
    }
}

#[test]
fn deterministic_random_paths_match_independent_static_overlap_bisection() {
    // The oracle calls the pre-existing static query and bisects a known clear/overlap
    // bracket. It does not use sweep intervals or duplicate the new moving-query code.
    let mut random = Samples(0xc8ae_3f17);
    let mut edge_normal_count = 0;
    for sample in 0..400 {
        let rotation = Quat::from_axis_angle(random.vector().normalize(), random.scalar() * 6.0);
        let solid = Obb::new(
            random.vector() * 5.0,
            random.vector().abs() * 1.5 + Vec3::splat(0.03),
            rotation,
        );
        let direction = random.vector().normalize();
        let body = Aabb::from_center_size(
            solid.center - direction * 8.0,
            random.vector().abs() * 1.5 + Vec3::splat(0.1),
        );
        let delta = direction * 16.0;
        assert!(!solid.overlaps_aabb(&body));
        assert!(solid.overlaps_aabb(&translated(body, delta * 0.5)));
        let mut clear = 0.0;
        let mut overlap = 0.5;
        for _ in 0..28 {
            let midpoint = (clear + overlap) * 0.5;
            if solid.overlaps_aabb(&translated(body, delta * midpoint)) {
                overlap = midpoint;
            } else {
                clear = midpoint;
            }
        }
        let hit = body
            .sweep_obb(delta, &solid)
            .unwrap_or_else(|| panic!("missed sample {sample}: {body:?}, {solid:?}, {delta:?}"));
        assert!(
            (hit.t - overlap).abs() < 5e-6,
            "sample {sample}: {hit:?}, oracle {overlap}"
        );
        assert!(delta.dot(hit.normal) < -1e-5);
        assert_contact_geometry(body, delta, solid, hit);
        let axes = [rotation * Vec3::X, rotation * Vec3::Y, rotation * Vec3::Z];
        if hit.normal.abs().max_element() < 0.999
            && axes.iter().all(|axis| hit.normal.dot(*axis).abs() < 0.999)
        {
            edge_normal_count += 1;
        }

        // Translating a perpendicular path well outside both circumradii must miss.
        let tangent = direction.any_orthonormal_vector();
        let miss = translated(body, tangent * 8.0);
        assert!(
            miss.sweep_obb(delta, &solid).is_none(),
            "false positive sample {sample}"
        );
    }
    assert!(
        edge_normal_count > 10,
        "random set must exercise cross-product separating axes"
    );
}
