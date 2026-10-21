use macroquad::prelude::*;

fn bounds(min: Vec3, max: Vec3) -> Aabb {
    Aabb::from_min_max(min, max)
}
fn point(p: Vec3) -> Aabb {
    bounds(p, p)
}

#[test]
fn empty_duplicates_and_snapshot_ownership() {
    let empty = Octree::new([]).unwrap();
    assert!(empty.is_empty());
    assert_eq!(empty.bounds(), None);
    assert!(empty
        .query_ray(&Ray::new(Vec3::ZERO, Vec3::X), f32::INFINITY)
        .is_empty());
    let old = point(Vec3::ZERO);
    let mut input = vec![(u32::MAX, old), (4, old), (4, point(Vec3::ONE))];
    let tree = Octree::new(input.clone()).unwrap();
    input[0].1 = point(Vec3::splat(99.0));
    assert_eq!(tree.len(), 2);
    assert_eq!(tree.query_aabb(&old), vec![u32::MAX]);
    assert_eq!(tree.query_aabb(&point(Vec3::ONE)), vec![4]);
    assert_eq!(tree.bounds(), Some(bounds(Vec3::ZERO, Vec3::ONE)));
    assert_eq!(tree.clone().query_aabb(&old), vec![u32::MAX]);
}

#[test]
fn validation_is_all_or_nothing() {
    let good = point(Vec3::ZERO);
    for bad in [
        bounds(Vec3::ONE, Vec3::ZERO),
        point(Vec3::splat(f32::NAN)),
        point(Vec3::splat(f32::INFINITY)),
        point(Vec3::splat(f32::NEG_INFINITY)),
    ] {
        assert!(Octree::new([(1, bad), (1, good)]).is_none());
        assert!(Octree::new([(1, good), (2, bad)]).is_none());
        assert!(Octree::new([(1, good)])
            .unwrap()
            .query_aabb(&bad)
            .is_empty());
    }
    let tree = Octree::new([(1, good)]).unwrap();
    for limit in [f32::NAN, -1.0, f32::NEG_INFINITY] {
        assert!(tree
            .query_ray(&Ray::new(Vec3::ZERO, Vec3::X), limit)
            .is_empty());
    }
    for ray in [
        Ray::new(Vec3::splat(f32::INFINITY), Vec3::X),
        Ray::new(Vec3::ZERO, Vec3::splat(f32::NAN)),
    ] {
        assert!(tree.query_ray(&ray, 1.0).is_empty());
    }
}

#[test]
fn octants_split_planes_spanning_boxes_and_inclusive_touches() {
    let mut entries = vec![(100, bounds(Vec3::splat(-10.0), Vec3::splat(10.0)))];
    for i in 0..27 {
        let p = vec3(
            (i % 3) as f32 - 1.0,
            ((i / 3) % 3) as f32 - 1.0,
            (i / 9) as f32 - 1.0,
        );
        entries.push((i, point(p)));
    }
    let tree = Octree::new(entries.clone()).unwrap();
    for (_, b) in entries {
        let mut expected = vec![100];
        for i in 0..27 {
            let p = vec3(
                (i % 3) as f32 - 1.0,
                ((i / 3) % 3) as f32 - 1.0,
                (i / 9) as f32 - 1.0,
            );
            if b.contains(p) {
                expected.push(i);
            }
        }
        expected.sort_unstable();
        assert_eq!(tree.query_aabb(&b), expected);
    }
}

#[test]
fn ray_parameterization_tangencies_points_and_order() {
    let tree = Octree::new([
        (20, bounds(vec3(1., -1., -1.), vec3(2., 1., 1.))),
        (3, point(vec3(4., 1., 1.))),
        (0, point(vec3(-1., 1., 1.))),
    ])
    .unwrap();
    let ray = Ray::new(vec3(0., 1., 1.), Vec3::X * 2.);
    assert_eq!(tree.query_ray(&ray, 0.5), vec![20]);
    assert_eq!(tree.query_ray(&ray, 2.), vec![3, 20]);
    assert_eq!(tree.query_ray(&ray, f32::INFINITY), vec![3, 20]);
    assert_eq!(
        tree.query_ray(&Ray::new(vec3(4., 1., 1.), Vec3::ZERO), f32::INFINITY),
        vec![3]
    );
    assert_eq!(
        tree.query_ray(&Ray::new(vec3(4., 1., 1.), Vec3::X), 0.),
        vec![3]
    );
    assert_eq!(
        tree.query_ray(&Ray::new(vec3(0., 1., 1.), -Vec3::X), 1.),
        vec![0]
    );
}

#[test]
fn extreme_coordinates_subnormal_directions_and_degenerate_planes() {
    let tiny = f32::from_bits(1);
    let entries = [
        (1, point(Vec3::X * tiny)),
        (2, point(Vec3::X * f32::MAX)),
        (3, point(-Vec3::X * f32::MAX)),
        (4, bounds(vec3(-1., 2., -1.), vec3(1., 2., 1.))),
    ];
    let tree = Octree::new(entries).unwrap();
    assert_eq!(tree.bounds().unwrap().max.x, f32::MAX);
    assert_eq!(
        tree.query_ray(&Ray::new(Vec3::ZERO, Vec3::X * tiny), 1.),
        vec![1]
    );
    assert_eq!(
        tree.query_ray(&Ray::new(Vec3::ZERO, Vec3::X * tiny), f32::INFINITY),
        vec![1, 2]
    );
    assert_eq!(tree.query_ray(&Ray::new(Vec3::ZERO, Vec3::Y), 2.), vec![4]);
    assert_eq!(tree.query_aabb(&entries[2].1), vec![3]);
}

#[test]
fn deterministic_integer_scene_matches_independent_overlap_and_axis_ray_oracles() {
    let mut seed = 73_u32;
    let mut next = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed % 201) as i32 - 100
    };
    let entries: Vec<_> = (0..700)
        .map(|i| {
            let lo = vec3(next() as f32, next() as f32, next() as f32);
            let size = vec3(
                (next() % 13).abs() as f32,
                (next() % 13).abs() as f32,
                (next() % 13).abs() as f32,
            );
            (i, bounds(lo, lo + size))
        })
        .collect();
    let trees: Vec<_> = [0, 1, 4, 16, u32::MAX]
        .into_iter()
        .map(|d| Octree::with_max_depth(entries.clone(), d).unwrap())
        .collect();
    for _ in 0..2000 {
        let origin = vec3(next() as f32, next() as f32, next() as f32);
        let end = origin
            + vec3(
                (next() % 20).abs() as f32,
                (next() % 20).abs() as f32,
                (next() % 20).abs() as f32,
            );
        let expected: Vec<_> = entries
            .iter()
            .filter(|(_, b)| (0..3).all(|a| b.min[a] <= end[a] && b.max[a] >= origin[a]))
            .map(|(i, _)| *i)
            .collect();
        for tree in &trees {
            assert_eq!(tree.query_aabb(&bounds(origin, end)), expected);
        }
        for axis in 0..3 {
            let mut direction = Vec3::ZERO;
            direction[axis] = if next() >= 0 { 2. } else { -2. };
            let mut low = origin;
            let mut high = origin;
            low[axis] += direction[axis].min(0.) * 13.;
            high[axis] += direction[axis].max(0.) * 13.;
            let expected: Vec<_> = entries
                .iter()
                .filter(|(_, b)| (0..3).all(|a| b.min[a] <= high[a] && b.max[a] >= low[a]))
                .map(|(i, _)| *i)
                .collect();
            for tree in &trees {
                assert_eq!(tree.query_ray(&Ray::new(origin, direction), 13.), expected);
            }
        }
    }
}

#[test]
fn arbitrary_rays_match_flat_bounds_filter_across_depths() {
    let entries: Vec<_> = (0..300)
        .map(|i| {
            let p = vec3(
                (i % 17) as f32 - 8.,
                (i % 13) as f32 - 6.,
                (i % 11) as f32 - 5.,
            );
            (i, bounds(p, p + Vec3::splat((i % 5) as f32)))
        })
        .collect();
    let flat = Octree::with_max_depth(entries.clone(), 0).unwrap();
    let tree = Octree::new(entries.clone()).unwrap();
    let mut grid = UniformGrid::new(2.).unwrap();
    grid.rebuild(entries);
    for i in 0..4000 {
        let ray = Ray::new(
            vec3(
                (i % 37) as f32 - 18.,
                (i % 29) as f32 - 14.,
                (i % 23) as f32 - 11.,
            ),
            vec3(
                (i % 7) as f32 - 3.,
                (i % 11) as f32 - 5.,
                (i % 13) as f32 - 6.,
            ),
        );
        let t = if i % 2 == 0 {
            f32::INFINITY
        } else {
            (i % 19) as f32 / 3.
        };
        let expected = flat.query_ray(&ray, t);
        assert_eq!(tree.query_ray(&ray, t), expected);
        assert_eq!(grid.query_ray(&ray, t), expected);
    }
}
