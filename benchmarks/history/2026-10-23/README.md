# Performance baseline and grid optimization — 23 October 2026

The fork now has 14 deterministic CPU workloads, an immutable measurement runner,
and a roadmap requirement to preserve comparable before/after evidence over time.
This maintenance task leaves the scheduled-round counters N=5, M=0, D=2 unchanged.

## Measurement

Release builds, optional `gltf`, Rust 1.99.1, WSL2 Linux on Intel Core i9-12900K,
24 logical CPUs, execution pinned to logical CPU 2. Each case has three process
executions with nine samples each, 75 ms warmup and calibrated batches targeting
30 ms. Values are batch-average CPU ns/op; p95 is across those averages, not
individual-operation tail latency. GPU performance, browser FPS, end-to-end game
frame times, process memory and physical devices were not benchmarked.

The primary pair is [20261023T113849.826124Z-baseline-final-0f0c5b40](20261023T113849.826124Z-baseline-final-0f0c5b40.json) and
[20261023T114056.102676Z-optimized-final-51b63ba8](20261023T114056.102676Z-optimized-final-51b63ba8.json). Both use identical workload, harness,
compiler, locked dependencies, host/affinity and sampling settings. Their source
SHA-256 values are `19ad5cb7d773d5cc26f10d507afb48d43e272da4f6cebf47ee464cb654c52416` and
`67d3c0b1b0b1500104dd99c8ade446b40d80747f3c1b24af4e78a4b544f13d2e` respectively. They are working-tree measurements
against Git `ace27f86f38c441abfeb40c0370eb53371c889ef`; the manifest and archive identify the exact
uncommitted code rather than attributing timings to that older commit alone.

## Change and decision

`UniformGrid` previously built a BTreeSet while visiting every selected cell.
Queries now collect a compact vector, sort/deduplicate it, and switch immediately
to the existing exact bounds scan when collected memberships reach the object
count. The check occurs before appending a cell. Candidate storage stays O(objects),
and sorted, unique, inclusive-boundary results and public APIs are preserved.

The optimization is retained for the substantial measured grid-query improvements.
This is **not an all-green performance comparison**: the independent
`grid.sparse_full_scan` control is about 11% slower. It scans an unchanged fixture
vector with `Aabb::overlaps`, without calling the optimized UniformGrid query.
Running just the two scan controls reproduced the shift (+11.13%
for sparse; +3.66% for dense). Compiler/code placement or
allocator/locality effects are hypotheses, not an established cause. The regression
flag remains active, and `--fail-on-regression` intentionally fails this pair.
No harness/inlining change was made to conceal the result; smaller gains should
be interpreted in that context, and there is no game-FPS improvement claim.

Benchmark comparison (negative change is faster)

| Benchmark | Baseline median ns/op | Candidate median ns/op | Median change | p95 change | Noise guard | Result |
|---|---:|---:|---:|---:|---:|---|
| math.aabb_ray | 10.53 | 10.54 | +0.12% | -0.68% | 5.00% | within_noise |
| math.sphere_sweep_aabb | 199.11 | 197.42 | -0.85% | -0.45% | 5.00% | within_noise |
| grid.sparse_aabb | 1781.52 | 1463.57 | -17.85% | -20.68% | 8.74% | improvement |
| grid.sparse_full_scan | 1993.62 | 2215.23 | +11.12% | +9.74% | 5.48% | regression |
| grid.sparse_tiny | 135.66 | 117.27 | -13.55% | -13.27% | 5.00% | improvement |
| grid.empty_aabb | 36.24 | 38.79 | +7.03% | +4.35% | 8.99% | within_noise |
| grid.sparse_ray | 461.17 | 429.82 | -6.80% | -7.06% | 5.00% | improvement |
| grid.rebuild | 601370.11 | 600717.83 | -0.11% | +0.30% | 5.00% | within_noise |
| grid.dense_aabb | 329819.05 | 3139.63 | -99.05% | -99.05% | 5.00% | improvement |
| grid.dense_full_scan | 2293.45 | 2387.17 | +4.09% | +3.63% | 6.96% | within_noise |
| grid.multicell_aabb | 2806220.75 | 436.83 | -99.98% | -99.98% | 5.00% | improvement |
| gltf.animation_sample | 101.57 | 101.53 | -0.04% | -0.96% | 5.00% | within_noise |
| gltf.skin_positions | 95763.69 | 95888.09 | +0.13% | +0.44% | 5.00% | within_noise |
| gltf.skin_normals | 126961.36 | 126432.99 | -0.42% | +0.24% | 5.00% | within_noise |

CPU microbenchmarks only; these measurements do not establish frame time, GPU throughput, or end-to-end game performance.


## Traceability and rejected work

- Every run record retains source and fixture hashes, compiler/environment,
  configuration, raw samples, output checksums and distributions. The full local
  archives/logs remain in `experiments/macroquad-coffee-break/artifacts/performance-oct23/runs/`.
- `dependencies.lock` preserves the exact Cargo lockfile. Each `.engine.patch`
  preserves that candidate's grid change against the untouched original engine,
  with line endings normalized for review (exact bytes remain in `source.zip`).
  The harness and runner are committed alongside this history; full `source.zip`
  archives are retained locally for exact working-tree recovery.
- The initial HashSet trial improved dense queries but regressed ordinary queries
  by 57–130%; it was rejected. Its run, comparison and patch remain in this folder.
- Early pilot runs precede the provenance hardening. Their source archives omitted
  some embedded binaries and used exact OS memory accounting in compatibility.
  A later pilot comparison correctly refused an 8 KB WSL accounting difference.
  Final runs include all embedded inputs, bind the recorded compiler to Cargo,
  and retain raw memory while explicitly comparing capacity rounded to 64 MiB.
  The final pair was measured afresh after these fixes; incompatible series were
  not combined or edited retrospectively.
- Filtered control runs have different sampling scope and are a separate series.
  Their [comparison](20261023T114640.355135Z-optimized-control-0ebfaae8.comparison.json)
  does not replace or clear the original full-suite warning.

## Validation

All eleven validation steps pass on the final engine/harness source: 29 Python
tooling tests; 585 default and 808 glTF/video native tests;
207 default and 437 feature doctests; native builds,
feature WebAssembly release build, all examples and default/feature Clippy.
Graphics tests use Xvfb/software OpenGL. Existing repository warnings remain;
Clippy reports no warning in the new benchmark, new tests or changed grid file.
The 18 targeted grid tests include exact budget/exceedance, fallback entries,
dense overlap, replacement, sorted identity and full-scan differential checks.
Independent review found no grid correctness or bounded-memory issue.

The final source was restored after the isolated baseline diagnostic and its
complete source fingerprint rechecked against the measured/validated optimization.
Full logs and the executed validation script are in the local round evidence.
No game sources, assets, CI workflows or remote deployment were changed.
