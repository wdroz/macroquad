use macroquad::prelude::*;

fn bounds(min: [f32; 3], max: [f32; 3]) -> Aabb {
    Aabb::from_min_max(Vec3::from_array(min), Vec3::from_array(max))
}

fn point(point: Vec3) -> Aabb {
    Aabb::from_min_max(point, point)
}

#[test]
fn checked_cell_size_and_empty_queries() {
    for invalid in [0.0, -0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(UniformGrid::new(invalid).is_none());
    }
    for size in [f32::from_bits(1), 0.5, 1.0, f32::MAX] {
        let grid = UniformGrid::new(size).unwrap();
        assert_eq!(grid.cell_size(), size);
        assert!(grid.is_empty());
        assert_eq!(grid.len(), 0);
        assert!(grid.query_aabb(&point(Vec3::ZERO)).is_empty());
        assert!(grid.query_ray(&Ray::default(), f32::INFINITY).is_empty());
    }
}

#[test]
fn spanning_boxes_are_found_in_every_cell_once() {
    let mut grid = UniformGrid::new(1.0).unwrap();
    assert!(grid.insert(9, bounds([-2.5; 3], [2.5; 3])));
    assert!(grid.insert(3, bounds([-1.5; 3], [1.5; 3])));
    for x in -2..=2 {
        for y in -2..=2 {
            for z in -2..=2 {
                let at = vec3(x as f32, y as f32, z as f32);
                let expected = if at.abs().max_element() <= 1.5 {
                    vec![3, 9]
                } else {
                    vec![9]
                };
                assert_eq!(grid.query_aabb(&point(at)), expected);
            }
        }
    }
    assert_eq!(grid.query_aabb(&bounds([-4.0; 3], [4.0; 3])), vec![3, 9]);
}

#[test]
fn negative_cells_and_face_edge_corner_touches_are_inclusive() {
    let mut grid = UniformGrid::new(1.0).unwrap();
    grid.insert(0, bounds([-2.0; 3], [-1.0; 3]));
    grid.insert(1, bounds([0.0; 3], [1.0; 3]));
    for at in [vec3(-1.0, -1.5, -1.5), vec3(-1.0, -1.0, -1.5), -Vec3::ONE] {
        assert_eq!(grid.query_aabb(&point(at)), vec![0]);
    }
    for at in [vec3(1.0, 0.5, 0.5), vec3(1.0, 1.0, 0.5), Vec3::ONE] {
        assert_eq!(grid.query_aabb(&point(at)), vec![1]);
    }
    assert!(grid.query_aabb(&point(Vec3::splat(-0.5))).is_empty());
    assert_eq!(grid.query_aabb(&bounds([-1.0; 3], [0.0; 3])), vec![0, 1]);
}

#[test]
fn cell_candidates_are_filtered_against_actual_stored_bounds() {
    let mut grid = UniformGrid::new(10.0).unwrap();
    grid.insert(8, bounds([0.0; 3], [1.0; 3]));
    grid.insert(2, bounds([5.0; 3], [6.0; 3]));
    assert_eq!(grid.query_aabb(&point(Vec3::splat(0.5))), vec![8]);
    assert!(grid.query_aabb(&point(Vec3::splat(3.0))).is_empty());
    assert!(grid
        .query_ray(&Ray::new(Vec3::splat(3.0), Vec3::X), 1.0)
        .is_empty());
}

#[test]
fn same_index_replaces_all_previous_cell_memberships() {
    let mut grid = UniformGrid::new(1.0).unwrap();
    let origin = point(Vec3::ZERO);
    grid.insert(u32::MAX, bounds([-2.0; 3], [2.0; 3]));
    grid.insert(u32::MAX, point(Vec3::splat(8.0)));
    assert_eq!(grid.len(), 1);
    assert!(grid.query_aabb(&origin).is_empty());
    assert_eq!(grid.query_aabb(&point(Vec3::splat(8.0))), vec![u32::MAX]);
    grid.insert(u32::MAX, origin);
    assert!(grid.query_aabb(&point(Vec3::splat(8.0))).is_empty());
    assert_eq!(grid.query_aabb(&origin), vec![u32::MAX]);
}

#[test]
fn invalid_insert_and_query_do_not_normalize_or_mutate() {
    let mut grid = UniformGrid::new(1.0).unwrap();
    grid.insert(7, point(Vec3::ZERO));
    for invalid in [
        bounds([1.0, 0.0, 0.0], [0.0; 3]),
        bounds([0.0, f32::NAN, 0.0], [1.0; 3]),
        bounds([0.0; 3], [1.0, 1.0, f32::INFINITY]),
        bounds([f32::NEG_INFINITY, 0.0, 0.0], [1.0; 3]),
    ] {
        assert!(!grid.insert(7, invalid));
        assert!(!grid.insert(8, invalid));
        assert!(grid.query_aabb(&invalid).is_empty());
        assert_eq!(grid.len(), 1);
        assert_eq!(grid.query_aabb(&point(Vec3::ZERO)), vec![7]);
    }
}

#[test]
fn clear_rebuild_skip_invalids_and_keep_last_valid_duplicate() {
    let mut grid = UniformGrid::new(2.0).unwrap();
    grid.insert(90, point(Vec3::ZERO));
    let invalid = bounds([1.0; 3], [0.0; 3]);
    assert_eq!(
        grid.rebuild([
            (4, point(Vec3::ZERO)),
            (4, point(Vec3::ONE)),
            (4, invalid),
            (2, point(Vec3::ONE)),
            (8, invalid)
        ]),
        2
    );
    assert_eq!(grid.query_aabb(&point(Vec3::ONE)), vec![2, 4]);
    assert!(grid.query_aabb(&point(Vec3::ZERO)).is_empty());
    grid.clear();
    assert!(grid.is_empty());
    assert_eq!(grid.cell_size(), 2.0);
    assert!(grid.query_aabb(&point(Vec3::ONE)).is_empty());
    grid.insert(1, point(Vec3::ONE));
    assert_eq!(grid.rebuild([]), 0);
    assert!(grid.query_ray(&Ray::default(), f32::INFINITY).is_empty());
}

#[test]
fn ray_limit_is_inclusive_parameter_and_results_are_sorted_by_id() {
    let mut grid = UniformGrid::new(1.0).unwrap();
    grid.insert(9, bounds([1.0, -1.0, -1.0], [2.0, 1.0, 1.0]));
    grid.insert(2, bounds([3.0, -1.0, -1.0], [4.0, 1.0, 1.0]));
    grid.insert(1, bounds([-2.0, -1.0, -1.0], [-1.0, 1.0, 1.0]));
    let ray = Ray::new(Vec3::ZERO, Vec3::X * 2.0);
    assert!(grid
        .query_ray(&ray, f32::from_bits(0.5_f32.to_bits() - 1))
        .is_empty());
    assert_eq!(grid.query_ray(&ray, 0.5), vec![9]);
    assert_eq!(grid.query_ray(&ray, 1.5), vec![2, 9]);
    assert_eq!(grid.query_ray(&ray, f32::INFINITY), vec![2, 9]);
    assert_eq!(
        grid.query_ray(&Ray::new(Vec3::ZERO, -Vec3::X * 2.0), 0.5),
        vec![1]
    );
}

#[test]
fn ray_boundary_tangencies_and_stationary_points() {
    let mut grid = UniformGrid::new(1.0).unwrap();
    grid.insert(0, bounds([0.0; 3], [1.0; 3]));
    for ray in [
        Ray::new(vec3(-1.0, 1.0, 1.0), Vec3::X),
        Ray::new(-Vec3::ONE, Vec3::ONE),
        Ray::new(Vec3::ONE, Vec3::ONE),
        Ray::new(Vec3::ZERO, -Vec3::ONE),
        Ray::new(Vec3::ONE, Vec3::ZERO),
    ] {
        assert_eq!(grid.query_ray(&ray, 1.0), vec![0]);
        assert_eq!(grid.query_ray(&ray, f32::INFINITY), vec![0]);
    }
    for direction in [Vec3::ZERO, Vec3::X, -Vec3::ONE] {
        assert_eq!(
            grid.query_ray(&Ray::new(Vec3::ONE, direction), 0.0),
            vec![0]
        );
        assert!(grid
            .query_ray(&Ray::new(Vec3::splat(2.0), direction), 0.0)
            .is_empty());
    }
    assert!(grid
        .query_ray(&Ray::new(Vec3::splat(2.0), Vec3::ZERO), f32::INFINITY)
        .is_empty());
}

#[test]
fn invalid_rays_and_limits_return_empty() {
    let mut grid = UniformGrid::new(1.0).unwrap();
    grid.insert(0, bounds([-1.0; 3], [1.0; 3]));
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(grid
            .query_ray(&Ray::new(vec3(invalid, 0.0, 0.0), Vec3::X), 1.0)
            .is_empty());
        assert!(grid
            .query_ray(&Ray::new(Vec3::ZERO, vec3(0.0, invalid, 0.0)), 1.0)
            .is_empty());
    }
    for invalid in [-1.0, f32::NEG_INFINITY, f32::NAN] {
        assert!(grid.query_ray(&Ray::default(), invalid).is_empty());
    }
}

#[test]
fn large_objects_queries_and_index_overflow_use_complete_fallbacks() {
    let mut grid = UniformGrid::new(1.0).unwrap();
    let all = bounds([-f32::MAX; 3], [f32::MAX; 3]);
    grid.insert(1, all);
    grid.insert(2, point(Vec3::splat(f32::MAX)));
    grid.insert(3, point(Vec3::ZERO));
    assert_eq!(grid.query_aabb(&point(Vec3::ZERO)), vec![1, 3]);
    assert_eq!(grid.query_aabb(&point(Vec3::splat(f32::MAX))), vec![1, 2]);
    assert_eq!(grid.query_aabb(&all), vec![1, 2, 3]);
    assert_eq!(
        grid.query_ray(
            &Ray::new(Vec3::splat(-f32::MAX), Vec3::splat(f32::MAX)),
            2.0
        ),
        vec![1, 2, 3]
    );
    grid.insert(1, point(Vec3::splat(-5.0)));
    grid.insert(3, all);
    assert_eq!(grid.query_aabb(&point(Vec3::ZERO)), vec![3]);
    assert_eq!(grid.len(), 3);
}

#[test]
fn subnormal_cell_sizes_and_directions_keep_nonzero_motion() {
    let tiny = f32::from_bits(1);
    let mut grid = UniformGrid::new(tiny).unwrap();
    grid.insert(1, point(Vec3::X * tiny));
    grid.insert(2, point(Vec3::X));
    let ray = Ray::new(Vec3::ZERO, Vec3::X * tiny);
    assert_eq!(grid.query_ray(&ray, 1.0), vec![1]);
    assert_eq!(grid.query_ray(&ray, f32::INFINITY), vec![1, 2]);
    assert_eq!(grid.query_aabb(&point(Vec3::X)), vec![2]);
}

#[test]
fn ray_segment_bounds_preserve_precision_near_cell_edges() {
    let mut grid = UniformGrid::new(1.0).unwrap();
    grid.insert(1, point(Vec3::X));
    // Computing this endpoint in f32 rounds to one, but it is just below one.
    let ray = Ray::new(
        vec3(0.5, 0.0, 0.0),
        vec3(f32::from_bits(0.5_f32.to_bits() - 1), 0.0, 0.0),
    );
    assert!(grid.query_ray(&ray, 1.0).is_empty());
    assert_eq!(
        grid.query_ray(&ray, f32::from_bits(1.0_f32.to_bits() + 1)),
        vec![1]
    );
}

#[test]
fn finite_ray_endpoint_rejection_is_consistent_with_fallback_queries() {
    let tiny = f32::from_bits(1);
    for size in [tiny, 1.0, f32::MAX] {
        let mut grid = UniformGrid::new(size).unwrap();
        grid.insert(1, point(Vec3::splat(-tiny)));
        grid.insert(2, point(Vec3::ZERO));
        grid.insert(3, point(Vec3::ONE));
        grid.insert(4, point(Vec3::splat(2147483648.0)));
        assert_eq!(
            grid.query_ray(&Ray::new(Vec3::ONE, -Vec3::ONE), 1.0),
            vec![2, 3]
        );
        assert_eq!(
            grid.query_ray(&Ray::new(Vec3::splat(-f32::MAX), Vec3::ONE), f32::MAX),
            vec![1, 2]
        );
    }
}
