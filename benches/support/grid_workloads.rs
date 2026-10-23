//! Additional spatial workloads. Fixture construction and reference checks are
//! outside timing; the original suite retains the ordinary sparse/dense controls.

use super::*;
use macroquad::math::Octree;

pub(super) fn cases(settings: &Settings) -> usize {
    let mut selected = 0;
    let entries = (0..64_u32)
        .map(|id| {
            let center = vec3(
                (id % 8) as f32 * 4.0 - 14.0,
                (id % 3) as f32 * 0.25,
                (id / 8) as f32 * 4.0 - 14.0,
            );
            (id, Aabb::from_center_size(center, Vec3::splat(0.75)))
        })
        .collect::<Vec<_>>();
    let mut grid = UniformGrid::new(1.0).unwrap();
    assert_eq!(grid.rebuild(entries.iter().copied()), entries.len());
    let mut entry_fp = Fingerprint::new();
    entry_fp.f32(1.0);
    for (id, bounds) in &entries {
        entry_fp.bytes(&id.to_le_bytes());
        entry_fp.aabb(bounds);
    }

    // Each query selects 16^3 cells, but the scene has only 64 entries. Include
    // occupied and entirely empty regions as separate cases so their costs stay
    // visible instead of averaging one pathological case into the ordinary lane.
    for empty in [false, true] {
        let queries = (0..32)
            .map(|i| {
                let center = vec3(
                    ((i * 3) % 8) as f32 * 4.0 - 14.0,
                    0.0,
                    ((i * 5) % 8) as f32 * 4.0 - 14.0,
                ) + if empty { Vec3::X * 200.0 } else { Vec3::ZERO };
                Aabb::from_center_size(center, Vec3::splat(14.5))
            })
            .collect::<Vec<_>>();
        let mut fp = entry_fp;
        for query in &queries {
            let expected = full_scan(&entries, query);
            assert_eq!(expected.is_empty(), empty);
            assert_eq!(grid.query_aabb(query), expected);
            fp.aabb(query);
        }
        selected += run(
            settings,
            if empty {
                "grid.small_wide_empty"
            } else {
                "grid.small_wide_aabb"
            },
            if empty {
                "sparse64-empty4096cells-query32-v1"
            } else {
                "sparse64-occupied4096cells-query32-v1"
            },
            fp,
            queries.len() as u64,
            || {
                let mut digest = Digest::default();
                for query in black_box(&queries) {
                    let result = black_box(grid.query_aabb(black_box(query)));
                    digest.ids(&result);
                }
                digest
            },
        ) as usize;
    }

    // The original tiny-query control has 4096 entries. This one exposes any
    // added selection overhead for ordinary single-cell queries in small scenes.
    let points = entries
        .iter()
        .map(|(_, bounds)| {
            let center = (bounds.min + bounds.max) * 0.5;
            Aabb::from_min_max(center, center)
        })
        .collect::<Vec<_>>();
    let mut fp = entry_fp;
    for (i, point) in points.iter().enumerate() {
        assert_eq!(grid.query_aabb(point), vec![entries[i].0]);
        fp.aabb(point);
    }
    selected += run(
        settings,
        "grid.small_tiny",
        "sparse64-hit-point64-v1",
        fp,
        points.len() as u64,
        || {
            let mut digest = Digest::default();
            for point in black_box(&points) {
                let result = black_box(grid.query_aabb(black_box(point)));
                digest.ids(&result);
            }
            digest
        },
    ) as usize;

    let rays = (0..32)
        .map(|i| {
            let center = vec3(
                ((i * 3) % 8) as f32 * 4.0 - 14.0,
                0.25,
                ((i * 5) % 8) as f32 * 4.0 - 14.0,
            );
            Ray::new(center - vec3(7.25, 0.0, 5.25), vec3(1.0, 0.0, 0.75))
        })
        .collect::<Vec<_>>();
    // A depth-zero octree independently scans every stored bound using the same
    // documented ray predicate, bypassing all grid cell-selection decisions.
    let scan = Octree::with_max_depth(entries.iter().copied(), 0).unwrap();
    let mut fp = entry_fp;
    fp.f32(14.0);
    for ray in &rays {
        assert_eq!(grid.query_ray(ray, 14.0), scan.query_ray(ray, 14.0));
        fp.vec3(ray.origin);
        fp.vec3(ray.direction);
    }
    selected += run(
        settings,
        "grid.small_wide_ray",
        "sparse64-finite-wide-ray32-v1",
        fp,
        rays.len() as u64,
        || {
            let mut digest = Digest::default();
            for ray in black_box(&rays) {
                let result = black_box(grid.query_ray(black_box(ray), black_box(14.0)));
                digest.ids(&result);
            }
            digest
        },
    ) as usize;

    // Repeated small movements retain cell memberships but must refresh exact
    // bounds. Two phases finish in the same state on every workload iteration.
    let updates = [0.1, -0.1].map(|offset| {
        (0..4096_u32)
            .map(|id| {
                let center = vec3(
                    (id % 64) as f32 * 4.0 + 2.0 + offset,
                    2.0,
                    (id / 64) as f32 * 4.0 + 2.0,
                );
                (id, Aabb::from_center_size(center, Vec3::splat(0.5)))
            })
            .collect::<Vec<_>>()
    });
    let mut moving = UniformGrid::new(4.0).unwrap();
    moving.rebuild(updates[1].iter().copied());
    let mut fp = Fingerprint::new();
    fp.f32(4.0);
    for phase in &updates {
        for &(id, bounds) in phase {
            assert!(moving.insert(id, bounds));
            fp.bytes(&id.to_le_bytes());
            fp.aabb(&bounds);
        }
        for i in (0..phase.len()).step_by(127) {
            assert_eq!(moving.query_aabb(&phase[i].1), vec![phase[i].0]);
        }
    }
    selected += run(
        settings,
        "grid.same_cell_update",
        "sparse4096-same-cell-two-phase-v1",
        fp,
        (updates[0].len() * updates.len()) as u64,
        || {
            let mut digest = Digest::default();
            for phase in black_box(&updates) {
                for &(id, bounds) in phase {
                    let accepted = black_box(moving.insert(black_box(id), black_box(bounds)));
                    digest.value(u64::from(accepted));
                    digest.hits += u64::from(accepted);
                }
            }
            black_box(&moving);
            digest
        },
    ) as usize;

    // Large finite limits and infinite rays already use the full scan. Keep
    // this separate control to detect regressions in shared ray-query plumbing.
    let fixture = grid_fixture(false);
    let scan = Octree::with_max_depth(fixture.entries.iter().copied(), 0).unwrap();
    let rays = fixture
        .queries
        .iter()
        .enumerate()
        .map(|(i, query)| {
            (
                Ray::new(
                    (query.min + query.max) * 0.5 - Vec3::X * 200.0,
                    vec3(1.0, 0.002, 0.001),
                ),
                if i % 2 == 0 {
                    1_000_000.0
                } else {
                    f32::INFINITY
                },
            )
        })
        .collect::<Vec<_>>();
    let mut fp = fixture.fingerprint;
    for (ray, limit) in &rays {
        assert_eq!(
            fixture.grid.query_ray(ray, *limit),
            scan.query_ray(ray, *limit)
        );
        fp.vec3(ray.origin);
        fp.vec3(ray.direction);
        fp.f32(*limit);
    }
    selected += run(
        settings,
        "grid.long_ray_scan",
        "sparse4096-long-finite-and-infinite-ray32-v1",
        fp,
        rays.len() as u64,
        || {
            let mut digest = Digest::default();
            for (ray, limit) in black_box(&rays) {
                let result = black_box(fixture.grid.query_ray(black_box(ray), black_box(*limit)));
                digest.ids(&result);
            }
            digest
        },
    ) as usize;

    let tree = Octree::new(fixture.entries.iter().copied()).unwrap();
    let mut tree_fp = fixture.fingerprint;
    tree_fp.bytes(&tree.max_depth().to_le_bytes());
    for ray in &fixture.rays {
        assert_eq!(tree.query_ray(ray, 8.0), scan.query_ray(ray, 8.0));
    }
    selected += run(
        settings,
        "octree.sparse_ray",
        "sparse4096-depth16-bounded-ray32-v1",
        tree_fp,
        fixture.rays.len() as u64,
        || {
            let mut digest = Digest::default();
            for ray in black_box(&fixture.rays) {
                let result = black_box(tree.query_ray(black_box(ray), black_box(8.0)));
                digest.ids(&result);
            }
            digest
        },
    ) as usize;

    let mut scan_fp = fp;
    scan_fp.bytes(&scan.max_depth().to_le_bytes());
    selected += run(
        settings,
        "octree.flat_long_ray",
        "sparse4096-depth0-long-finite-and-infinite-ray32-v1",
        scan_fp,
        rays.len() as u64,
        || {
            let mut digest = Digest::default();
            for (ray, limit) in black_box(&rays) {
                let result = black_box(scan.query_ray(black_box(ray), black_box(*limit)));
                digest.ids(&result);
            }
            digest
        },
    ) as usize;

    let tiny_rays = (0..32)
        .map(|i| {
            let offset = (i / 4) as f32 * 0.01;
            match i % 4 {
                0 => (Ray::new(vec3(-2.0, offset, 0.0), Vec3::X * 2.0), 1.0),
                1 => (Ray::new(vec3(-2.0, 2.0 + offset, 0.0), Vec3::X), 4.0),
                2 => (
                    Ray::new(vec3(0.0, offset, 0.0), vec3(-0.0, 0.0, -0.0)),
                    f32::INFINITY,
                ),
                _ => (Ray::new(vec3(2.0, offset, 0.0), -Vec3::X), 0.0),
            }
        })
        .collect::<Vec<_>>();
    for empty in [false, true] {
        let bounds = Aabb::from_center_size(Vec3::ZERO, Vec3::ONE);
        let entries = if empty { Vec::new() } else { vec![(9, bounds)] };
        let tree = Octree::new(entries).unwrap();
        let mut fp = Fingerprint::new();
        fp.bytes(&(tree.len() as u64).to_le_bytes());
        fp.bytes(&tree.max_depth().to_le_bytes());
        if !empty {
            fp.bytes(&9_u32.to_le_bytes());
            fp.aabb(&bounds);
        }
        for (index, (ray, limit)) in tiny_rays.iter().enumerate() {
            let expected = if !empty && index % 2 == 0 {
                vec![9]
            } else {
                Vec::new()
            };
            assert_eq!(tree.query_ray(ray, *limit), expected);
            fp.vec3(ray.origin);
            fp.vec3(ray.direction);
            fp.f32(*limit);
        }
        selected += run(
            settings,
            if empty {
                "octree.empty_ray"
            } else {
                "octree.tiny_ray"
            },
            if empty {
                "empty-finite-infinite-stationary-ray32-v1"
            } else {
                "box1-finite-infinite-stationary-ray32-hit16-v1"
            },
            fp,
            tiny_rays.len() as u64,
            || {
                let mut digest = Digest::default();
                for (ray, limit) in black_box(&tiny_rays) {
                    let result = black_box(tree.query_ray(black_box(ray), black_box(*limit)));
                    digest.ids(&result);
                }
                digest
            },
        ) as usize;
    }
    selected
}
