# Animation, skinning and grid updates — 23 October 2026

This performance pass reduces measured CPU time for grid rebuilds by **25.92%**,
animation sampling by **21.49%**, and position-only skinning by **12.39%**.
The compatible 14-case comparison reports no regression. Skinning with normals
measured 8.16% lower, but its 8.26% noise guard makes that result **inconclusive**.
These are CPU microbenchmarks, not game frame-rate or GPU measurements.

The engine changes and six new regression tests are committed in
[`6ae238d`](https://github.com/wdroz/macroquad/commit/6ae238da085c8b9695a80c2e66362f1cdfc05579).
This user-requested maintenance leaves the scheduled counters N=5, M=0, D=2 unchanged.

## What changed

- Animation sampling interpolates translation and scale directly into `Vec3`
  instead of allocating a temporary vector for every transform channel. Rotation
  no longer computes unused component interpolation. Morph weights keep their
  owned vectors; f64 interpolation and quaternion behavior are preserved.
- CPU skinning computes each validated joint inverse once, retains inverse
  transposes only when normals exist, and specializes the vertex loop for the
  presence of normals. All matrix, index, weight and overflow checks remain,
  including unused joints and calls without normals.
- Grid insertion uses one map update instead of removing and reinserting the
  entry. Moving within the same cell range, or remaining in the fallback set,
  updates exact bounds while retaining memberships. The maintained workload
  measures rebuilds; same-cell replacement has correctness tests but no separate
  timing claim in this series.

## Measurement and interpretation

Release builds with `gltf`, Rust 1.99.1, locked dependencies, WSL2 Linux on an
Intel Core i9-12900K with 24 logical CPUs, pinned to CPU 2. Each workload has three
independent process executions, nine samples per execution, 75 ms warmup and
calibrated batches targeting 30 ms. Builds and tests ran separately from timing.
The harness, fixtures, output checksums, compiler, features, dependencies,
machine and sampling settings match. The strict `--fail-on-regression` comparison
exited successfully.

Times are batch-average CPU nanoseconds per operation. p95 is across those
averages, not individual-operation tail latency. The noise guard is the greater
of 5% and three times the runner's observed relative dispersion; it is an
engineering threshold, not a statistical significance test.

| Benchmark | Baseline median ns/op | Candidate median ns/op | Median change | p95 change | Noise guard | Result |
|---|---:|---:|---:|---:|---:|---|
| math.aabb_ray | 10.60 | 10.47 | -1.21% | -1.02% | 5.00% | within_noise |
| math.sphere_sweep_aabb | 201.50 | 197.15 | -2.15% | -1.46% | 5.00% | within_noise |
| grid.sparse_aabb | 1473.03 | 1288.87 | -12.50% | -8.13% | 10.82% | improvement |
| grid.sparse_full_scan | 2237.82 | 1989.84 | -11.08% | -10.88% | 6.41% | improvement |
| grid.sparse_tiny | 119.68 | 116.93 | -2.30% | -0.95% | 5.00% | within_noise |
| grid.empty_aabb | 39.55 | 39.17 | -0.95% | -1.60% | 6.53% | within_noise |
| grid.sparse_ray | 439.64 | 430.30 | -2.12% | -3.18% | 5.09% | within_noise |
| grid.rebuild | 608844.03 | 451015.10 | -25.92% | -27.29% | 5.70% | improvement |
| grid.dense_aabb | 3179.80 | 3220.03 | +1.27% | +0.75% | 5.56% | within_noise |
| grid.dense_full_scan | 2424.25 | 2309.51 | -4.73% | -7.43% | 5.34% | within_noise |
| grid.multicell_aabb | 448.13 | 439.95 | -1.82% | -0.94% | 5.00% | within_noise |
| gltf.animation_sample | 103.45 | 81.21 | -21.49% | -21.69% | 5.00% | improvement |
| gltf.skin_positions | 97982.50 | 85844.47 | -12.39% | -11.60% | 5.96% | improvement |
| gltf.skin_normals | 130690.59 | 120020.96 | -8.16% | -9.28% | 8.26% | within_noise |

Negative changes mean less CPU time. Skin workloads evaluate 4,096 vertices with
64 joints and four nonzero influences; animation samples a four-channel clip;
grid rebuild inserts 4,096 sparse entries.

The unchanged sparse full-scan control also measures 11.08% lower, and the
unchanged grid query path measures 12.50% lower. We retain those observations
without attributing them to a new query algorithm. Code placement, allocation
locality and environmental variation are possible explanations, not established
causes. This result does not resolve or erase the earlier
[full-scan regression](../2026-10-23/README.md). The primary optimization claims
above concern the three directly changed workloads; the normals result remains
inconclusive. No browser, physical-device, GPU, process-memory or game-FPS gain
has been established.

## Evidence and reproduction

- [Baseline record](20261023T124320.434842Z-baseline-skinning-grid-92893711.json),
  measured at Git `d9076a15eb29277339a00a3b5d8bdfb101d07e41`.
- [Candidate record](20261023T124952.517715Z-optimized-skinning-grid-fad30ebf.json),
  measured at Git `6ae238da085c8b9695a80c2e66362f1cdfc05579`.
- Generated [Markdown comparison](20261023T124952.517715Z-optimized-skinning-grid-fad30ebf.comparison.md)
  and [JSON comparison](20261023T124952.517715Z-optimized-skinning-grid-fad30ebf.comparison.json).
- Normalized [engine patch](20261023T124952.517715Z-optimized-skinning-grid-fad30ebf.engine.patch)
  and exact [dependency lockfile](dependencies.lock).
- [Validation results](validation.json), including commands, counts and warnings.

Baseline source SHA-256:
`67d3c0b1b0b1500104dd99c8ade446b40d80747f3c1b24af4e78a4b544f13d2e`.
Candidate source SHA-256:
`950a9b2be5f249464bca0083cbf9e50e8285e9ba4d45d2a80aa1fa7667060eb6`.
The baseline source fingerprint matches the previous pass's final optimized
source, so this pass starts after that grid-query optimization.

WSL Git reports existing CRLF/LF differences in this Windows checkout; the
recorded per-file hashes and source archives identify the exact measured bytes.
Full immutable archives, build logs and raw process output remain locally in
`experiments/macroquad-coffee-break/artifacts/performance-followup-oct23/runs/`.
The compact records retain every raw timing sample. No historical result was
overwritten or rebaselined after a regression.

From the fork, reproduce each revision with its recorded source archive and
lockfile, then run the same settings:

```sh
CARGO_TARGET_DIR=/home/wdroz/macroquad-performance-target CARGO_BUILD_JOBS=2 \
  taskset -c 2 python3 scripts/benchmark.py run --features gltf \
  --label reproduction --repeats 3 --samples 9 --warmup-ms 75 --sample-ms 30
```

Compare the saved records without rebuilding:

```sh
python3 scripts/benchmark.py compare \
  benchmarks/history/2026-10-23-followup/20261023T124320.434842Z-baseline-skinning-grid-92893711.json \
  benchmarks/history/2026-10-23-followup/20261023T124952.517715Z-optimized-skinning-grid-fad30ebf.json \
  --fail-on-regression
```

## Validation

All eleven checks pass: 29 benchmark-tooling tests, native default/feature
builds, 587 default and 814 glTF/video tests, 207 default and 437 feature doctests,
all examples, both all-target Clippy runs, and the glTF/video WebAssembly release
build. Existing warning kinds are unchanged; none was introduced by this pass.
Native graphics tests use Xvfb/software OpenGL. Browser runtime, Metal and
physical phones were not tested in this CPU-only change.

The six added regressions cover bitwise scalar-reference skinning parity with
one through eight influences, zero weights/normals, reflected/nonuniform joint
transforms, invalid unused inputs, empty meshes, deformation overflow, exact
animation interpolation, and cell/fallback replacements with updated bounds.
Independent read-only review found no blocking correctness issue. The engine
checkpoint was pushed before the full measurement; this report preserves the
subsequent validation and performance evidence. No game sources, assets,
dependency versions, CI workflows or remote deployment were changed.
