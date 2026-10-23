# One-hour performance maintenance — 23 October 2026

This user-requested continuation expands the CPU suite from 14 to 35 workloads,
records every complete run in Markdown, and measures further engine changes.
The timed work began at 13:10:17 UTC. This report is updated as each trial finishes;
individual measurements and comparisons are immutable.

The retained v2 candidate reduces skinning CPU time **34.63%** without normals
and **24.21%** with normals, large-clip sampling **32.45%**, unchanged-pose
application **91.54%**, and the pathological wide small-scene queries **89.78% to
99.75%**. External sphere-to-mesh rejection drops from 9,809 ns to 4.4 ns by
checking the existing cached mesh bounds first. These are workload-specific
results, not frame-rate claims. The unchanged external mesh AABB control still
flags a **5.00%** slowdown; this is explicitly **not an all-green comparison**.

## Measurement rules

The `macroquad-cpu-v2` series starts at engine/tooling commit `639c429`, after the
[previous performance pass](../2026-10-23-followup/README.md). The new workload
definitions and harness fingerprint make it incompatible with v1. Its additional
controls cover small-scene wide/empty/tiny grid queries, same-cell updates, long
rays, triangle surfaces, mesh construction, large animation clips, morphing and
changed/unchanged poses. Construction is outside query timing; the construction
workload explicitly includes ownership of its input vectors.

All v2 runs use release builds with `gltf`, Rust 1.99.1, the same locked
dependencies, and WSL2 Linux on an Intel Core i9-12900K, pinned to logical CPU 2.
Each workload has three process executions, nine samples per execution, 75 ms
warmup and calibrated batches targeting 30 ms. Compilation and correctness
checks run separately from timing. Strict compatibility verifies workloads,
output checksums, source harness, toolchain, features, dependencies, environment
and sampling options. Engine source hashes may differ.

Numbers are batch-average CPU nanoseconds per operation; p95 is across these
averages. The guard is the greater of 5% and three times observed relative
dispersion, an engineering threshold rather than a significance test. These
results do not measure game FPS, GPU throughput, browser/device performance or
process memory. The [earlier unchanged full-scan slowdown](../2026-10-23/README.md)
remains part of the historical record; a new suite does not resolve it.

## Trials

1. [v2 baseline](20261023T131654.020764Z-hour-v2-baseline-26801258.md): exact source
   SHA-256 `d2f0c1e84161c35a3ab4a3986655e2da80f14b62c72074ae0e82b8194b7f2c47`.
2. [Round 1 comparison](20261023T132429.837237Z-hour-v2-round1-7ad1729a.comparison.md):
   cached animation slots, lazy pose staging, fused morph validation/bounds,
   scene-sized grid query limits, and whole-mesh query rejection. Wide small-scene
   grid queries, external mesh queries, large clips and unchanged poses improve,
   but grid rebuilds regress **7.94%**, local mesh AABB queries **23.17%**, and
   interleaved local mesh AABB queries **24.23%**. This candidate is rejected as a
   whole. Morph changes show no improvement beyond noise and are removed; useful
   correctness tests remain. These regression flags are retained unchanged.
3. [Round 2 comparison](20261023T133237.980569Z-hour-v2-round2-6596e9dd.comparison.md):
   removes the morph change, specializes grid insertion, isolates the mesh AABB
   scan from its new root rejection, and simplifies skin-weight validation with
   an equivalent IEEE-754 bit predicate. Local mesh queries and rebuilds return
   within their guards; skinning improves **34.01%** without normals and **23.68%**
   with normals. Sparse grid AABB queries now regress **18.62%**.
4. [Unchanged round 2 confirmation](20261023T133502.904893Z-hour-v2-round2-confirmation-ed5b0863.comparison.md):
   reproduces the sparse-query regression at **17.95%**. The unchanged full-scan
   control also flags **6.04%** slower. These results are retained; the sparse
   query needs further work before the candidate is accepted. Mesh and glTF gains
   remain, while morph controls remain within noise after reverting that trial.
5. [Round 3 comparison](20261023T133742.859823Z-hour-v2-round3-21b573db.comparison.md):
   keeps a constant-specialized insertion helper while restoring the query's
   runtime-limit calling shape. Rebuilds are **1.17%** slower and sparse AABB
   queries **6.02%** slower, both within their guards. The unchanged full-scan
   control is now **19.36%** faster, while unchanged local mesh AABB scans regress
   **22.79%** and **27.11%**. These shifts show sensitivity beyond the edited
   algorithm; code generation/placement and allocation locality are hypotheses,
   not established causes. The mesh AABB shortcut and helper are removed entirely
   because their local-query cost is not reliably acceptable. Sphere rejection
   remains, with consistent local controls. No timing record is discarded.
6. [Round 4 comparison](20261023T134111.232190Z-hour-v2-round4-f0c1921e.comparison.md):
   restores the original mesh AABB implementation. Sparse grid AABB queries still
   regress **21.25%**, and the unchanged external mesh AABB control flags **5.02%**.
   Rather than retain complicated cell-helper specialization without consistent
   evidence, the next trial restores the original insertion/cell-range code and
   adds the scene-size limit only when selecting a query range. Scenes with at
   least 4096 entries and empty scenes skip the added volume arithmetic.
7. [Round 5 retained candidate](20261023T134433.243483Z-hour-v2-round5-df10b705.comparison.md):
   uses that minimal query-only filter. All grid controls are within their guards,
   but sparse queries are **8.65%** slower under an **11.85%** guard and rebuilds
   **4.55%** slower under a **5%** guard; neither proves unchanged performance.
   The unchanged external mesh AABB control is **5.004%** slower, just over the
   guard, and remains flagged. The unchanged full-scan control is **27.44%**
   faster; this is not attributed to a new scan algorithm. The candidate retains
   the directly measured grid, sphere-query and glTF gains with these stated
   limits. No mesh AABB optimization or morph production change is retained.

## Retained evidence

Every run has a linked Markdown summary and compact JSON containing all original
samples and source/environment metadata. Candidate comparisons and normalized
engine patches are retained alongside the exact
[v2 dependency lockfile](macroquad-cpu-v2.dependencies.lock). The complete local
evidence, including exact source archives, untracked sources, compiler output and
raw process logs, is under
`experiments/macroquad-coffee-break/artifacts/performance-hour-oct23/runs/`.
Windows/WSL Git line-ending reports differ; exact file hashes and archives define
the measured bytes. No historical run is overwritten.

To compare saved compatible records without rebuilding:

```sh
python3 scripts/benchmark.py compare BASELINE.json CANDIDATE.json --fail-on-regression
```

To reproduce from the recorded source archive and lockfile:

```sh
CARGO_TARGET_DIR=/home/wdroz/macroquad-performance-target CARGO_BUILD_JOBS=2 \
  taskset -c 2 python3 scripts/benchmark.py run --features gltf \
  --label reproduction --repeats 3 --samples 9 --warmup-ms 75 --sample-ms 30
```

## Validation

Round 1 passes all eleven checks: 31 benchmark-tooling tests, native default and
glTF/video builds, 590 default tests, 822 glTF/video tests, 207 default doctests,
437 feature doctests, all examples, both all-target Clippy runs, and the glTF/video
WebAssembly release build. Existing warning kinds are unchanged. Graphics tests
use Xvfb/software OpenGL; these CPU changes have no physical-device runtime claim.
Later trials receive focused checks before timing and final validation before
commit. This maintenance leaves scheduled counters N=5, M=0, D=2 unchanged.
