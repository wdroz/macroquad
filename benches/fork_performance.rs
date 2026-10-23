//! Deterministic, headless CPU workloads. See docs/performance.md for recording
//! comparable runs. Change workload IDs when inputs or measured work change.

use macroquad::math::{vec3, Aabb, Ray, Sphere, UniformGrid, Vec3};
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

#[cfg(feature = "gltf")]
#[path = "support/animation_fixture.rs"]
mod animation_fixture;

#[cfg(feature = "gltf")]
#[path = "support/gltf_workloads.rs"]
mod gltf_workloads;
#[path = "support/grid_workloads.rs"]
mod grid_workloads;
#[path = "support/mesh_workloads.rs"]
mod mesh_workloads;

const SUITE_VERSION: &str = "macroquad-cpu-v2";

struct Settings {
    samples: usize,
    warmup_ms: u64,
    sample_ms: u64,
    filter: String,
    list: bool,
}

impl Settings {
    fn parse() -> Self {
        let mut settings = Self {
            samples: 9,
            warmup_ms: 75,
            sample_ms: 30,
            filter: String::new(),
            list: false,
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--bench" => {} // Added by cargo bench for harness=false targets.
                "--list" => settings.list = true,
                "--filter" => settings.filter = args.next().expect("--filter requires text"),
                "--samples" => {
                    settings.samples = args
                        .next()
                        .expect("--samples requires a count")
                        .parse()
                        .expect("invalid sample count")
                }
                "--warmup-ms" | "--warmup" => {
                    settings.warmup_ms = args
                        .next()
                        .expect("--warmup-ms requires milliseconds")
                        .parse()
                        .expect("invalid warmup milliseconds")
                }
                "--sample-ms" => {
                    settings.sample_ms = args
                        .next()
                        .expect("--sample-ms requires milliseconds")
                        .parse()
                        .expect("invalid sample milliseconds")
                }
                "--help" | "-h" => {
                    println!("fork_performance [--samples 9] [--warmup-ms 75] [--sample-ms 30] [--filter substring] [--list]");
                    std::process::exit(0);
                }
                _ => panic!("unrecognized argument: {arg}"),
            }
        }
        assert!(
            (3..=101).contains(&settings.samples),
            "--samples must be 3..=101"
        );
        assert!(
            settings.warmup_ms <= 10_000,
            "--warmup-ms must be 0..=10000"
        );
        assert!(
            (1..=5_000).contains(&settings.sample_ms),
            "--sample-ms must be 1..=5000"
        );
        settings
    }
}

// FNV-1a of canonical little-endian inputs: independent of std HashMap seeds.
#[derive(Clone, Copy)]
struct Fingerprint(u64);
impl Fingerprint {
    fn new() -> Self {
        Self(0xcbf29ce484222325)
    }
    fn bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    fn f32(&mut self, value: f32) {
        self.bytes(&value.to_bits().to_le_bytes());
    }
    fn vec3(&mut self, value: Vec3) {
        for component in value.to_array() {
            self.f32(component);
        }
    }
    fn aabb(&mut self, value: &Aabb) {
        self.vec3(value.min);
        self.vec3(value.max);
    }
}

// Lightweight output digest. Whole returned allocations are black-boxed; the
// checksum spot-checks large floating-point outputs to avoid timing a hash pass.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct Digest {
    checksum: u64,
    hits: u64,
}
impl Digest {
    fn value(&mut self, value: u64) {
        self.checksum = self.checksum.wrapping_mul(31).wrapping_add(value);
    }
    fn f32(&mut self, value: f32) {
        self.value(u64::from(value.to_bits()));
    }
    fn vec3(&mut self, value: Vec3) {
        for component in value.to_array() {
            self.f32(component);
        }
    }
    fn ids(&mut self, ids: &[u32]) {
        self.hits += ids.len() as u64;
        self.value(ids.len() as u64);
        for &id in ids {
            self.value(u64::from(id));
        }
    }
}

fn batch(iterations: u64, work: &mut impl FnMut() -> Digest) -> (Duration, Digest) {
    let start = Instant::now();
    let mut total = Digest::default();
    for _ in 0..iterations {
        let digest = black_box(work());
        total.checksum = total.checksum.wrapping_add(digest.checksum);
        total.hits = total.hits.wrapping_add(digest.hits);
    }
    (start.elapsed(), total)
}

fn check_digest(actual: Digest, expected: Digest, iterations: u64) {
    assert_eq!(
        actual,
        Digest {
            checksum: expected.checksum.wrapping_mul(iterations),
            hits: expected.hits.wrapping_mul(iterations),
        },
        "benchmark output changed between iterations"
    );
}

fn run(
    settings: &Settings,
    benchmark: &str,
    workload: &str,
    fingerprint: Fingerprint,
    operations: u64,
    mut work: impl FnMut() -> Digest,
) -> bool {
    if !benchmark.contains(&settings.filter) {
        return false;
    }
    // IDs consist only of ASCII letters, digits, '.', '-' and '_'.
    let metadata = format!(
        "\"schema_version\":1,\"suite_version\":\"{SUITE_VERSION}\",\"backend\":\"cpu\",\"benchmark\":\"{benchmark}\",\"workload\":\"{workload}\",\"fixture_fingerprint\":\"{:016x}\",\"operations_per_iteration\":{operations}", fingerprint.0);
    if settings.list {
        println!("{{\"event\":\"case\",{metadata}}}");
        return true;
    }
    let expected = black_box(work());
    let warmup_start = Instant::now();
    while warmup_start.elapsed() < Duration::from_millis(settings.warmup_ms) {
        check_digest(black_box(work()), expected, 1);
    }
    // Calibrate using complete workload iterations, then retain the same batch
    // size for every recorded sample. No fixture construction is timed.
    let target = Duration::from_millis(settings.sample_ms);
    let mut iterations = 1_u64;
    loop {
        let (elapsed, digest) = batch(iterations, &mut work);
        check_digest(digest, expected, iterations);
        if elapsed >= target || iterations >= 1 << 30 {
            break;
        }
        iterations *= 2;
    }
    let mut samples = Vec::with_capacity(settings.samples);
    for _ in 0..settings.samples {
        let (elapsed, digest) = batch(iterations, &mut work);
        check_digest(digest, expected, iterations);
        samples.push(elapsed.as_secs_f64() * 1e9 / iterations as f64 / operations as f64);
    }
    let mut sorted = samples.clone();
    sorted.sort_by(f64::total_cmp);
    let median = if sorted.len() % 2 == 0 {
        (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) * 0.5
    } else {
        sorted[sorted.len() / 2]
    };
    let values = samples
        .iter()
        .map(|v| format!("{v:.6}"))
        .collect::<Vec<_>>()
        .join(",");
    println!("{{\"event\":\"measurement\",{metadata},\"unit\":\"ns_per_op\",\"iterations\":{iterations},\"samples_ns\":[{values}],\"median_ns\":{median:.6},\"min_ns\":{:.6},\"max_ns\":{:.6},\"checksum_per_iteration\":\"{:016x}\",\"hits_per_iteration\":{}}}",
        sorted[0], sorted[sorted.len()-1], expected.checksum, expected.hits);
    true
}

struct GridFixture {
    entries: Vec<(u32, Aabb)>,
    queries: Vec<Aabb>,
    rays: Vec<Ray>,
    grid: UniformGrid,
    fingerprint: Fingerprint,
}

fn grid_fixture(dense: bool) -> GridFixture {
    let count = if dense { 1024 } else { 4096 };
    let cell_size = if dense { 1.0 } else { 4.0 };
    let mut entries = (0..count)
        .map(|index| {
            let center = if dense {
                vec3(
                    (index % 16) as f32 * 0.15 - 1.2,
                    ((index / 16) % 8) as f32 * 0.2 - 0.8,
                    (index / 128) as f32 * 0.15 - 0.6,
                )
            } else {
                vec3(
                    (index % 32) as f32 * 3.0 - 48.0,
                    ((index / 32) % 4) as f32 * 3.0 - 6.0,
                    (index / 128) as f32 * 3.0 - 48.0,
                )
            };
            let size = if dense {
                vec3(2.2, 1.8, 2.4)
            } else {
                vec3(1.2, 1.4, 1.6)
            };
            (index, Aabb::from_center_size(center, size))
        })
        .collect::<Vec<_>>();
    // Deliberately exercise the large-entry fallback alongside ordinary cells.
    if dense {
        for id in 0..4 {
            entries.push((
                count + id,
                Aabb::from_center_size(Vec3::splat(id as f32), Vec3::splat(100.0)),
            ));
        }
    }
    let queries = (0..32)
        .map(|i| {
            let center = if dense {
                vec3(
                    (i % 8) as f32 * 0.5 - 2.0,
                    (i % 4) as f32 * 0.6 - 1.2,
                    (i / 8) as f32 * 0.7 - 1.4,
                )
            } else {
                // Mix occupied and empty space, negative coordinates, and boundaries.
                vec3(
                    ((i * 13) % 32) as f32 * 3.0 - 47.7,
                    (i % 4) as f32 * 3.0 - 6.0,
                    ((i * 7) % 32) as f32 * 3.0 - 48.0,
                )
            };
            Aabb::from_center_size(
                center,
                if dense {
                    Vec3::splat(2.5)
                } else {
                    Vec3::splat(5.0)
                },
            )
        })
        .collect::<Vec<_>>();
    let rays = queries
        .iter()
        .map(|q| Ray::new((q.min + q.max) * 0.5 - Vec3::X * 2.0, vec3(1.0, 0.1, 0.2)))
        .collect::<Vec<_>>();
    let mut grid = UniformGrid::new(cell_size).unwrap();
    assert_eq!(grid.rebuild(entries.iter().copied()), entries.len());
    let mut fingerprint = Fingerprint::new();
    fingerprint.f32(cell_size);
    for (id, bounds) in &entries {
        fingerprint.bytes(&id.to_le_bytes());
        fingerprint.aabb(bounds);
    }
    for query in &queries {
        fingerprint.aabb(query);
    }
    for ray in &rays {
        fingerprint.vec3(ray.origin);
        fingerprint.vec3(ray.direction);
    }
    fingerprint.f32(8.0); // Finite ray limit.
    for query in &queries {
        assert_eq!(
            grid.query_aabb(query),
            full_scan(&entries, query),
            "grid must match complete scan before timing"
        );
    }
    GridFixture {
        entries,
        queries,
        rays,
        grid,
        fingerprint,
    }
}

fn full_scan(entries: &[(u32, Aabb)], query: &Aabb) -> Vec<u32> {
    // The fixture is sorted by caller ID, just like UniformGrid's output.
    entries
        .iter()
        .filter_map(|(id, bounds)| bounds.overlaps(query).then_some(*id))
        .collect()
}

fn math_cases(settings: &Settings) -> usize {
    let mut selected = 0;
    let fixtures = (0..256)
        .map(|i| {
            let center = vec3(
                (i % 16) as f32 * 0.25,
                ((i / 16) % 4) as f32 * 0.5,
                (i / 64) as f32 * 0.4,
            );
            let bounds = Aabb::from_center_size(center, vec3(1.5, 1.0, 1.8));
            let ray = Ray::new(
                center + vec3(-3.0, if i % 3 == 0 { 2.0 } else { 0.1 }, 0.2),
                vec3(1.0, 0.0, 0.05),
            );
            let sphere = Sphere::new(ray.origin, 0.3);
            (bounds, ray, sphere, vec3(5.0, -0.1, 0.0))
        })
        .collect::<Vec<_>>();
    let mut fp = Fingerprint::new();
    for (bounds, ray, sphere, delta) in &fixtures {
        fp.aabb(bounds);
        fp.vec3(ray.origin);
        fp.vec3(ray.direction);
        fp.vec3(sphere.center);
        fp.f32(sphere.radius);
        fp.vec3(*delta);
    }
    selected += run(
        settings,
        "math.aabb_ray",
        "mixed-ray-hit-miss-256-v1",
        fp,
        fixtures.len() as u64,
        || {
            let mut digest = Digest::default();
            for (bounds, ray, _, _) in black_box(&fixtures) {
                if let Some(hit) =
                    black_box(bounds.ray_intersect(black_box(ray.origin), black_box(ray.direction)))
                {
                    digest.hits += 1;
                    digest.f32(hit.t);
                    digest.vec3(hit.normal);
                } else {
                    digest.value(0);
                }
            }
            digest
        },
    ) as usize;
    selected += run(
        settings,
        "math.sphere_sweep_aabb",
        "mixed-sphere-sweep-256-v1",
        fp,
        fixtures.len() as u64,
        || {
            let mut digest = Digest::default();
            for (bounds, _, sphere, delta) in black_box(&fixtures) {
                if let Some(hit) =
                    black_box(sphere.sweep_aabb(black_box(*delta), black_box(bounds)))
                {
                    digest.hits += 1;
                    digest.f32(hit.t);
                    digest.vec3(hit.normal);
                } else {
                    digest.value(0);
                }
            }
            digest
        },
    ) as usize;
    selected
}

fn grid_cases(settings: &Settings) -> usize {
    let mut selected = 0;
    for dense in [false, true] {
        let fixture = grid_fixture(dense);
        let workload = if dense {
            "dense-1024-fallback4-query32-v1"
        } else {
            "sparse-4096-query32-v1"
        };
        selected += run(
            settings,
            if dense {
                "grid.dense_aabb"
            } else {
                "grid.sparse_aabb"
            },
            workload,
            fixture.fingerprint,
            fixture.queries.len() as u64,
            || {
                let mut digest = Digest::default();
                for query in black_box(&fixture.queries) {
                    let result = black_box(fixture.grid.query_aabb(black_box(query)));
                    digest.ids(&result);
                }
                digest
            },
        ) as usize;
        selected += run(
            settings,
            if dense {
                "grid.dense_full_scan"
            } else {
                "grid.sparse_full_scan"
            },
            workload,
            fixture.fingerprint,
            fixture.queries.len() as u64,
            || {
                let mut digest = Digest::default();
                for query in black_box(&fixture.queries) {
                    let result =
                        black_box(full_scan(black_box(&fixture.entries), black_box(query)));
                    digest.ids(&result);
                }
                digest
            },
        ) as usize;
        if !dense {
            for empty in [false, true] {
                let points = (0..32)
                    .map(|i| {
                        let bounds = fixture.entries[i * 127].1;
                        let center = (bounds.min + bounds.max) * 0.5;
                        let offset = if !empty {
                            Vec3::ZERO
                        } else if i % 2 == 0 {
                            Vec3::splat(1.2)
                        } else {
                            Vec3::X * 300.0
                        };
                        Aabb::from_center_size(center + offset, Vec3::ZERO)
                    })
                    .collect::<Vec<_>>();
                for point in &points {
                    let expected = full_scan(&fixture.entries, point);
                    assert_eq!(expected.len(), if empty { 0 } else { 1 });
                    assert_eq!(fixture.grid.query_aabb(point), expected);
                }
                let mut point_fp = fixture.fingerprint;
                for point in &points {
                    point_fp.aabb(point);
                }
                selected += run(
                    settings,
                    if empty {
                        "grid.empty_aabb"
                    } else {
                        "grid.sparse_tiny"
                    },
                    if empty {
                        "sparse4096-empty-point32-v1"
                    } else {
                        "sparse4096-hit-point32-v1"
                    },
                    point_fp,
                    points.len() as u64,
                    || {
                        let mut digest = Digest::default();
                        for point in black_box(&points) {
                            let result = black_box(fixture.grid.query_aabb(black_box(point)));
                            digest.ids(&result);
                        }
                        digest
                    },
                ) as usize;
            }
            selected += run(
                settings,
                "grid.sparse_ray",
                "sparse-4096-bounded-ray32-v1",
                fixture.fingerprint,
                fixture.rays.len() as u64,
                || {
                    let mut digest = Digest::default();
                    for ray in black_box(&fixture.rays) {
                        let result =
                            black_box(fixture.grid.query_ray(black_box(ray), black_box(8.0)));
                        digest.ids(&result);
                    }
                    digest
                },
            ) as usize;
            let mut grid = UniformGrid::new(fixture.grid.cell_size()).unwrap();
            selected += run(
                settings,
                "grid.rebuild",
                "sparse-4096-rebuild-v1",
                fixture.fingerprint,
                1,
                || {
                    let count =
                        black_box(grid.rebuild(black_box(&fixture.entries).iter().copied()));
                    black_box(&grid);
                    Digest {
                        checksum: count as u64,
                        hits: count as u64,
                    }
                },
            ) as usize;
        }
    }
    // Near-budget overlap: 128 entries each occupy 15^3 cells. This catches
    // candidate collectors that accumulate all 432000 duplicates before dedup.
    let entries = (0..128_u32)
        .map(|id| {
            let offset = Vec3::splat((id % 4) as f32 * 0.05);
            (
                id,
                Aabb::from_min_max(Vec3::splat(-7.0) + offset, Vec3::splat(7.0) + offset),
            )
        })
        .collect::<Vec<_>>();
    let query = Aabb::from_min_max(Vec3::splat(-7.0), Vec3::splat(7.0));
    let mut grid = UniformGrid::new(1.0).unwrap();
    grid.rebuild(entries.iter().copied());
    assert_eq!(grid.query_aabb(&query), full_scan(&entries, &query));
    let mut fp = Fingerprint::new();
    fp.f32(1.0);
    for (id, bounds) in &entries {
        fp.bytes(&id.to_le_bytes());
        fp.aabb(bounds);
    }
    fp.aabb(&query);
    selected += run(
        settings,
        "grid.multicell_aabb",
        "overlap128-cells3375-query1-v1",
        fp,
        1,
        || {
            let mut digest = Digest::default();
            let result = black_box(grid.query_aabb(black_box(&query)));
            digest.ids(&result);
            digest
        },
    ) as usize;
    selected
}

#[cfg(feature = "gltf")]
fn gltf_cases(settings: &Settings) -> usize {
    use macroquad::{
        gltf::{GltfAnimationData, GltfSkinPrimitive},
        math::{Mat4, Quat},
    };
    let mut selected = 0;
    let bytes = animation_fixture::glb();
    let clip = GltfAnimationData::from_bytes(&bytes)
        .unwrap()
        .clips
        .remove(0);
    let times = (0..64).map(|i| i as f32 / 32.0).collect::<Vec<_>>();
    let mut fp = Fingerprint::new();
    fp.bytes(&bytes);
    for &time in &times {
        fp.f32(time);
    }
    selected += run(
        settings,
        "gltf.animation_sample",
        "child-trs-weight-4-channel-time64-v1",
        fp,
        times.len() as u64,
        || {
            let mut digest = Digest::default();
            for &time in black_box(&times) {
                let sample = black_box(clip.sample(black_box(time)).unwrap());
                digest.hits += (sample.transforms.len() + sample.weights.len()) as u64;
                for (node, trs) in &sample.transforms {
                    digest.value(*node as u64);
                    digest.vec3(trs.translation);
                    digest.vec3(trs.scale);
                    for component in trs.rotation.to_array() {
                        digest.f32(component);
                    }
                }
                for (node, weights) in &sample.weights {
                    digest.value(*node as u64);
                    for &weight in weights {
                        digest.f32(weight);
                    }
                }
            }
            digest
        },
    ) as usize;

    // Scale the public CPU skin fixture's identity-bind/normalized-weight
    // structure to a small character mesh, with four nonzero influences.
    let vertices = 4096;
    let positions = (0..vertices)
        .map(|i| {
            vec3(
                (i % 64) as f32 / 32.0 - 1.0,
                (i / 64) as f32 / 32.0,
                (i % 7) as f32 * 0.05,
            )
        })
        .collect::<Vec<_>>();
    let normals = vec![Vec3::Y; vertices];
    let skin = GltfSkinPrimitive {
        joints: (0..vertices)
            .map(|i| {
                [
                    (i % 64) as u16,
                    ((i + 1) % 64) as u16,
                    ((i + 2) % 64) as u16,
                    ((i + 3) % 64) as u16,
                    0,
                    0,
                    0,
                    0,
                ]
            })
            .collect(),
        weights: vec![[0.4, 0.3, 0.2, 0.1, 0.0, 0.0, 0.0, 0.0]; vertices],
    };
    let matrices = (0..64)
        .map(|i| {
            Mat4::from_scale_rotation_translation(
                vec3(1.0 + i as f32 * 0.002, 1.0, 0.9),
                Quat::from_rotation_z(i as f32 * 0.01),
                vec3(0.0, i as f32 * 0.02, 0.0),
            )
        })
        .collect::<Vec<_>>();
    // Identity matrices must reproduce source geometry, independently of the
    // optimized path's output fingerprint. These checks are outside timing.
    let identity = skin
        .deform(&positions, &normals, &[Mat4::IDENTITY; 64])
        .unwrap();
    assert!(identity
        .positions
        .iter()
        .zip(&positions)
        .all(|(a, b)| a.abs_diff_eq(*b, 1e-5)));
    assert!(identity
        .normals
        .iter()
        .all(|v| v.abs_diff_eq(Vec3::Y, 1e-5)));
    let mut fp = Fingerprint::new();
    for position in &positions {
        fp.vec3(*position);
    }
    for normal in &normals {
        fp.vec3(*normal);
    }
    for joints in &skin.joints {
        for joint in joints {
            fp.bytes(&joint.to_le_bytes());
        }
    }
    for weights in &skin.weights {
        for &weight in weights {
            fp.f32(weight);
        }
    }
    for matrix in &matrices {
        for component in matrix.to_cols_array() {
            fp.f32(component);
        }
    }
    for with_normals in [false, true] {
        let input_normals = if with_normals {
            normals.as_slice()
        } else {
            &[]
        };
        let benchmark = if with_normals {
            "gltf.skin_normals"
        } else {
            "gltf.skin_positions"
        };
        let workload = if with_normals {
            "skin4096-joint64-influence4-normals-v1"
        } else {
            "skin4096-joint64-influence4-positions-v1"
        };
        selected += run(settings, benchmark, workload, fp, 1, || {
            let result = black_box(
                skin.deform(
                    black_box(&positions),
                    black_box(input_normals),
                    black_box(&matrices),
                )
                .unwrap(),
            );
            let mut digest = Digest {
                hits: result.positions.len() as u64,
                ..Default::default()
            };
            for index in [0, vertices / 2, vertices - 1] {
                digest.vec3(result.positions[index]);
                if with_normals {
                    digest.vec3(result.normals[index]);
                }
            }
            digest
        }) as usize;
    }
    selected
}

fn main() {
    let settings = Settings::parse();
    println!("{{\"event\":\"suite\",\"schema_version\":1,\"suite_version\":\"{SUITE_VERSION}\",\"backend\":\"cpu\",\"gltf\":{},\"samples\":{},\"warmup_ms\":{},\"sample_ms\":{}}}", cfg!(feature = "gltf"), settings.samples, settings.warmup_ms, settings.sample_ms);
    let mut selected = math_cases(&settings)
        + grid_cases(&settings)
        + grid_workloads::cases(&settings)
        + mesh_workloads::cases(&settings);
    #[cfg(feature = "gltf")]
    {
        selected += gltf_cases(&settings) + gltf_workloads::cases(&settings);
    }
    #[cfg(not(feature = "gltf"))]
    {
        let _ = &mut selected;
    }
    assert!(selected > 0, "filter matched no enabled benchmark cases");
}
