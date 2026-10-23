# One-hour performance maintenance — 23 October 2026

This user-requested continuation expands the CPU suite from 14 to 41 workloads
(35 in v2, then 41 in v3),
records every complete run in Markdown, and measures further engine changes.
The one-hour work window was 13:10:17–14:10:17 UTC. Individual measurements and comparisons
are immutable; the final report preserves every trial, including refusals.
The final engine and evidence commit is
[`b13d1c5`](https://github.com/wdroz/macroquad/commit/b13d1c5e5cc26622ae7d7cc41a519755e6a84abe).

The retained v2 candidate reduces skinning CPU time **34.63%** without normals
and **24.21%** with normals, large-clip sampling **32.45%**, unchanged-pose
application **91.54%**, and the pathological wide small-scene queries **89.78% to
99.75%**. External sphere-to-mesh rejection drops from 9,809 ns to 4.4 ns by
checking the existing cached mesh bounds first. These are workload-specific
results, not frame-rate claims. The unchanged external mesh AABB control still
flags a **5.00%** slowdown; this is explicitly **not an all-green comparison**.

The later compatible v3 trial reduces octree ray times **30.90–45.20%** and
combined pose application **17.68–85.78%**. It retains a separate **10.02%**
regression in an unchanged empty-result AABB query, an absolute cost of 15.16 ns.
Both series and every intermediate trial are retained below.

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

## Additional v3 series

Engine checkpoint `5d238e0` was committed and pushed with the complete v2 trial
history. Commit `6a5755e` adds six workloads and fixes Markdown CPU metadata on
Windows and platforms without Linux `/proc` fields. All 33 Python tooling tests
and all 41 benchmark setup checks pass. A probe using the actual Windows host
metadata also renders successfully.

The [v3 baseline](20261023T134829.332760Z-hour-v3-baseline-41172e2a.md) is measured
at `6a5755e` with the same WSL/compiler/sampling configuration. New controls cover
combined pose application with changed and unchanged transforms, plus ordinary,
flat, tiny and empty octree ray queries. Its new harness fingerprint requires a
fresh baseline. It does not overwrite, clear or directly compare against the v2
regression records above. The [v3 lockfile](macroquad-cpu-v3.dependencies.lock)
and full archived sources define this separate series.

The [first v3 candidate](20261023T135253.805328Z-hour-v3-prepared-rays-and-pose-3141fda6.md)
completed timing, but comparison was **refused** because added assertions in
`tests/support/gltf_animation_fixture.rs` changed the strict fixture fingerprint.
This graphics test is not timed by the CPU benchmark, but the guard deliberately
does not waive fixture differences. Its measurements remain diagnostic only.
The subsequent candidate restores that file's exact archived baseline bytes
while timing, then restores the additional assertions for final graphics-test
validation. Production code and benchmark workloads are unchanged between these
two candidates; no saved metadata is edited to manufacture compatibility.

The [compatible v3 comparison](20261023T135654.639285Z-hour-v3-frozen-fixtures-dea183a2.comparison.md)
retains the prepared-ray and transform-only combined-application changes:

| Targeted workload | Baseline ns/op | Candidate ns/op | Change |
|---|---:|---:|---:|
| Small-scene wide grid ray | 293.14 | 254.15 | -13.30% |
| Long grid ray scan | 30,913.95 | 27,764.61 | -10.19% |
| Ordinary octree ray | 378.20 | 261.35 | -30.90% |
| Flat octree long ray | 38,943.49 | 21,340.93 | -45.20% |
| Combined pose, changed | 9,504.80 | 7,824.44 | -17.68% |
| Combined pose, unchanged | 1,886.52 | 268.21 | -85.78% |

All six improvements exceed their noise guards. Ordinary grid rays and tiny
octree rays remain within noise; empty octrees improve 6.77%. The shared prepared
query computes finite endpoints/envelopes once, retaining f64 division, axis
order, zero/subnormal handling, inclusive bounds and endpoint-cancellation
protection. Transform-only combined samples call the existing atomic pose update
directly and leave a supplied morph instance untouched.

The **unchanged small-scene empty AABB query regresses 10.02%**, from 151.23 to
166.39 ns/op: an absolute increase of 15.16 ns. This is the only regression flag
in this v3 comparison and remains unresolved. Its AABB algorithm was not edited
in this trial; code generation/placement is a possible explanation, not a proven
cause. The candidate is retained for the substantial ray/pose gains with this
explicit cost; this is **not an all-green comparison**. The unchanged external
mesh AABB control improves 10.14%, likewise without a new AABB algorithm claim.
Neither observation erases the v2 flags or establishes a cross-version gain.

## Native Windows measurement

The [complete 41-case Windows run](20261023T140219.567078Z-hour-native-windows-smoke-f6fe1081.md)
uses Rust 1.95.0/MSVC and the same three-execution/nine-sample settings. It verifies
the release benchmark and automatic Markdown output on the actual Windows host,
including its native hardware metadata. It is a separate host/toolchain series;
its absolute times must not be compared directly with WSL results. Windows CPU
affinity was not configured or recorded by this runner, unlike the pinned WSL
series. No heavy build or test ran concurrently with either measured execution.

A Windows reference measurement is recorded after the candidate, restoring the
three production files from the archived v3 baseline while retaining the final
tests and identical harness. A `finally` block restores and verifies the exact
final source bytes afterward. This reference measures the final two optimizations
only; it cannot establish native Windows gains for earlier v2 changes. The order
is explicitly candidate then reference, and both records remain immutable.

The [Windows reference record](20261023T140513.517017Z-hour-native-windows-reference-baseline-71a2f458.md)
and [strict compatible comparison](20261023T140219.567078Z-hour-native-windows-smoke-f6fe1081.windows-comparison.md)
show wide/long grid rays **20.18%/10.20%** faster, ordinary/flat octree rays
**31.40%/13.02%** faster, and combined changed/unchanged poses **46.07%/87.69%**
faster, all beyond their guards. The reference was measured after the candidate;
the report makes no claim that it was a temporally prior run.

Four unchanged controls regress on Windows, so the native result is also
**not all-green**:

| Unchanged control | Reference ns/op | Candidate ns/op | Regression |
|---|---:|---:|---:|
| External mesh AABB | 1,927.29 | 2,051.98 | +6.47% |
| Local mesh AABB | 1,485.06 | 1,734.41 | +16.79% |
| Interleaved mesh AABB | 1,466.40 | 1,828.72 | +24.71% |
| Position-only skinning | 49,913.38 | 54,144.04 | +8.48% |

These flags remain unresolved. Different code generation/placement and execution
order are hypotheses, not proven causes. The measured candidate is retained with
these explicit platform-specific costs; it is not a universal performance gain.
The unchanged empty-result query control moves in the opposite direction on Windows
(-33.33%); this does not erase its WSL regression. Future work should isolate the
control sensitivity and measure representative game scenes before making a
frame-rate claim. After this reference run, final source identity was rechecked
and matches the already validated candidate exactly.

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

Round 1 passes [all eleven checks](validation-round1.json): 31 benchmark-tooling tests, native default and
glTF/video builds, 590 default tests, 822 glTF/video tests, 207 default doctests,
437 feature doctests, all examples, both all-target Clippy runs, and the glTF/video
WebAssembly release build. Existing warning kinds are unchanged. Graphics tests
use Xvfb/software OpenGL; these CPU changes have no physical-device runtime claim.
Later trials receive focused checks before timing.

The [final validation](validation-final.json) passes all eleven checks with
**33 Python tooling tests, 592 default Rust tests, 826 glTF/video tests, 207 default
doctests and 437 feature doctests**, both builds, both all-target Clippy runs,
all examples and the WebAssembly release build. Existing warning kinds are
unchanged. The restored windowed assertions verify populated morph weights,
bounds and zero uploads after transform-only success and failure. Two new ray
tests compare 42,800 numeric-edge/random finite-bit cases against the captured
legacy predicate.

[Native Windows validation](validation-windows.json) additionally passes
**35 release CPU tests** with Rust 1.95.0/MSVC, including the ray differential
tests, transform application, IEEE skin-weight edges, mesh pruning and grid
queries. Independent reviews found no correctness blocker in the retained
changes, and verified exact raw-record copies, source archives, lockfiles,
comparison recomputation and report links.

The [source identity check](final-source-identity.json) confirms every measured
engine and benchmark source byte matches the validated tree. The sole difference
is the restored windowed test assertions described above. Measured v3 source
SHA-256 is `089dc21fd54417e7fe346deb25d6ba33953ccf23ea035f2edeff6dc14660149b`;
validated source SHA-256 is
`ce43024e079aa80a01368d0ff958827a726ce83a14e07a3e9fa3649374e8c08c`.

This maintenance leaves scheduled counters N=5, M=0, D=2 unchanged. No public API,
dependency versions, CI workflow, game assets or deployment changed.
