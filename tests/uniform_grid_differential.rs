use macroquad::prelude::*;

#[test]
fn query_cell_budget_boundaries_match_complete_scans() {
    let entries = [
        (91, Aabb::from_min_max(Vec3::splat(0.5), Vec3::splat(0.5))),
        (
            2,
            Aabb::from_min_max(vec3(2.5, 0.5, 0.5), vec3(2.5, 0.5, 0.5)),
        ),
        (
            17,
            Aabb::from_min_max(vec3(4.5, 0.5, 0.5), vec3(4.5, 0.5, 0.5)),
        ),
        // Keep a huge fallback entry alongside indexed candidates; changing the
        // query traversal budget must not skip its bounds check.
        (7, Aabb::from_center_size(Vec3::ZERO, Vec3::splat(100.0))),
    ];
    let mut grid = UniformGrid::new(1.0).unwrap();
    assert_eq!(grid.rebuild(entries), 4);
    let scanned = Octree::with_max_depth(entries, 0).unwrap();
    for offset in [Vec3::ZERO, Vec3::splat(-3.0), Vec3::splat(200.0)] {
        // Four cells exactly match the entry count; five exceed it. The final
        // two ranges exercise cumulative volume across more than one axis.
        for extent in [
            vec3(4.0, 1.0, 1.0),
            vec3(5.0, 1.0, 1.0),
            vec3(2.0, 2.0, 1.0),
            Vec3::splat(2.0),
        ] {
            let min = offset + Vec3::splat(0.25);
            let max = offset + extent - Vec3::splat(0.25);
            let query = Aabb::from_min_max(min, max);
            let mut expected = entries
                .iter()
                .filter_map(|(id, bounds)| bounds.overlaps(&query).then_some(*id))
                .collect::<Vec<_>>();
            expected.sort_unstable();
            assert_eq!(grid.query_aabb(&query), expected);
            let ray = Ray::new(min, max - min);
            assert_eq!(grid.query_ray(&ray, 1.0), scanned.query_ray(&ray, 1.0));
        }
    }
    // Inclusive face endpoints remain visible across the len/len+1 boundary.
    let ray = Ray::new(Vec3::splat(0.5), Vec3::X);
    assert_eq!(grid.query_ray(&ray, 3.0), vec![2, 7, 91]);
    assert_eq!(grid.query_ray(&ray, 4.0), vec![2, 7, 17, 91]);
    grid.clear();
    assert!(grid.query_aabb(&entries[3].1).is_empty());
    assert!(grid.query_ray(&ray, 4.0).is_empty());
    assert!(grid.query_ray(&ray, f32::INFINITY).is_empty());
}

#[test]
fn replacements_with_unchanged_cells_refresh_exact_bounds() {
    let mut grid = UniformGrid::new(1.0).unwrap();
    // Keep the candidate count below the full-scan budget, so point queries
    // exercise retained cell memberships as well as the updated bounds.
    for id in [100, 101, 102] {
        assert!(grid.insert(
            id,
            Aabb::from_center_size(Vec3::splat(id as f32), Vec3::splat(0.25))
        ));
    }
    let before = Aabb::from_min_max(Vec3::splat(0.1), Vec3::splat(1.1));
    let after = Aabb::from_min_max(Vec3::splat(0.8), Vec3::splat(1.8));
    for bounds in [before, after, before, after] {
        assert!(grid.insert(7, bounds));
        assert_eq!(grid.len(), 4);
        for at in [Vec3::splat(0.2), Vec3::splat(0.9), Vec3::splat(1.7)] {
            let point = Aabb::from_min_max(at, at);
            let expected = if bounds.overlaps(&point) {
                vec![7]
            } else {
                Vec::new()
            };
            assert_eq!(grid.query_aabb(&point), expected);
            assert_eq!(grid.query_ray(&Ray::new(at, Vec3::X), 0.01), expected);
        }
    }
    // An invalid replacement must preserve the latest valid snapshot.
    assert!(!grid.insert(7, Aabb::from_min_max(Vec3::ONE, Vec3::ZERO)));
    let old_point = Aabb::from_min_max(Vec3::splat(0.2), Vec3::splat(0.2));
    let new_point = Aabb::from_min_max(Vec3::splat(1.7), Vec3::splat(1.7));
    assert!(grid.query_aabb(&old_point).is_empty());
    assert_eq!(grid.query_aabb(&new_point), vec![7]);
}

#[test]
fn fallback_replacements_refresh_bounds_and_keep_memberships() {
    let mut grid = UniformGrid::new(1.0).unwrap();
    assert!(grid.insert(
        90,
        Aabb::from_min_max(Vec3::splat(500.0), Vec3::splat(500.0))
    ));
    assert!(grid.insert(3, Aabb::from_min_max(Vec3::ZERO, Vec3::ZERO)));
    let near = Aabb::from_center_size(Vec3::ZERO, Vec3::splat(40.0));
    let far = Aabb::from_center_size(Vec3::splat(100.0), Vec3::splat(40.0));
    for bounds in [near, far, near, far] {
        assert!(grid.insert(17, bounds));
        assert_eq!(grid.len(), 3);
        for at in [Vec3::ZERO, Vec3::splat(100.0)] {
            let point = Aabb::from_min_max(at, at);
            let mut expected = if at == Vec3::ZERO {
                vec![3]
            } else {
                Vec::new()
            };
            if bounds.overlaps(&point) {
                expected.push(17);
            }
            assert_eq!(grid.query_aabb(&point), expected);
            assert_eq!(grid.query_ray(&Ray::new(at, Vec3::X), 0.1), expected);
        }
    }
    // Replacement across storage paths must still remove the old fallback.
    let final_point = Aabb::from_min_max(Vec3::splat(250.0), Vec3::splat(250.0));
    assert!(grid.insert(17, final_point));
    assert!(grid.query_aabb(&far).is_empty());
    assert_eq!(grid.query_aabb(&final_point), vec![17]);
}

#[test]
fn candidate_budget_boundaries_and_all_fallback_keep_exact_sorted_results() {
    // All three indexed entries occupy the same cell. Querying that cell reaches
    // the entry budget exactly, but only one stored AABB actually intersects.
    let mut grid = UniformGrid::new(10.0).unwrap();
    for (id, x) in [(90, 1.0), (3, 3.0), (17, 5.0)] {
        assert!(grid.insert(id, Aabb::from_center_size(Vec3::splat(x), Vec3::ONE)));
    }
    let at = Vec3::splat(3.0);
    let point = Aabb::from_min_max(at, at);
    assert_eq!(grid.query_aabb(&point), vec![3]);
    assert_eq!(grid.query_ray(&Ray::new(at, Vec3::ZERO), 0.0), vec![3]);

    // The first cell contributes two candidates below a budget of three. Both
    // appear in the next cell too, so appending that cell would exceed it.
    let mut repeated = UniformGrid::new(1.0).unwrap();
    for id in [9, 2] {
        assert!(repeated.insert(
            id,
            Aabb::from_min_max(Vec3::splat(0.25), vec3(1.25, 0.75, 0.75))
        ));
    }
    assert!(repeated.insert(0, Aabb::from_center_size(Vec3::splat(20.0), Vec3::ONE)));
    let from = Vec3::splat(0.5);
    let spanning = Aabb::from_min_max(from, vec3(1.5, 0.5, 0.5));
    assert_eq!(repeated.query_aabb(&spanning), vec![2, 9]);
    assert_eq!(
        repeated.query_ray(&Ray::new(from, Vec3::X), 1.0),
        vec![2, 9]
    );

    // There are no indexed candidates when every object uses the large-bounds
    // fallback. Keep filtering and ordering even for a query inside one cell.
    let mut fallback = UniformGrid::new(1.0).unwrap();
    for (id, x) in [(100, 0.0), (2, 0.0), (31, 100.0)] {
        assert!(fallback.insert(
            id,
            Aabb::from_center_size(Vec3::splat(x), Vec3::splat(40.0))
        ));
    }
    let origin = Aabb::from_min_max(Vec3::ZERO, Vec3::ZERO);
    assert_eq!(fallback.query_aabb(&origin), vec![2, 100]);
    assert_eq!(
        fallback.query_ray(&Ray::new(Vec3::ZERO, Vec3::X), 1.0),
        vec![2, 100]
    );
}

// The subnormal cell size puts these world-space boxes in the fallback path,
// giving ray queries the same exact intersection rules without cell selection.
#[test]
fn dense_cell_candidates_match_full_scan_after_replacements() {
    let mut indexed = UniformGrid::new(1.0).unwrap();
    let mut scanned = UniformGrid::new(f32::from_bits(1)).unwrap();
    let mut objects = Vec::new();
    for ordinal in 0..256_u32 {
        let id = u32::MAX - ordinal * 17;
        let center = vec3(
            (ordinal % 11) as f32 * 0.5 - 2.5,
            (ordinal % 7) as f32 * 0.5 - 1.5,
            (ordinal % 5) as f32 * 0.5 - 1.0,
        );
        let size = if ordinal % 31 == 0 {
            Vec3::splat(40.0)
        } else {
            Vec3::splat(1.5 + (ordinal % 3) as f32)
        };
        let bounds = Aabb::from_center_size(center, size);
        assert!(indexed.insert(id, bounds));
        assert!(scanned.insert(id, bounds));
        objects.push((id, bounds));
    }

    for round in 0..3 {
        for x in -6..=6 {
            for y in -3..=3 {
                let center = vec3(x as f32 * 0.75, y as f32 * 0.5, 0.25);
                let bounds = Aabb::from_center_size(center, vec3(2.5, 1.0, 0.5));
                let mut expected = objects
                    .iter()
                    .filter_map(|&(id, entry)| entry.overlaps(&bounds).then_some(id))
                    .collect::<Vec<_>>();
                expected.sort_unstable();
                assert_eq!(indexed.query_aabb(&bounds), expected);

                for direction in [Vec3::X, -Vec3::Y, vec3(1.0, 0.5, -0.25)] {
                    let ray = Ray::new(center, direction);
                    assert_eq!(indexed.query_ray(&ray, 3.0), scanned.query_ray(&ray, 3.0));
                }
            }
        }

        // Swap some entries between indexed cells and the large-object fallback,
        // and move other repeated IDs to make stale membership observable.
        for (ordinal, (id, bounds)) in objects.iter_mut().enumerate() {
            if ordinal % 5 == round {
                let size = if ordinal % 2 == 0 { 48.0 } else { 0.25 };
                *bounds = Aabb::from_center_size(
                    vec3(round as f32 * 2.0, ordinal as f32 * 0.125 - 8.0, 0.0),
                    Vec3::splat(size),
                );
                assert!(indexed.insert(*id, *bounds));
                assert!(scanned.insert(*id, *bounds));
            }
        }
    }
}
