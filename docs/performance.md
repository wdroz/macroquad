# Performance history

The [recorded runs](../benchmarks/history/README.md) include the first baseline,
bounded grid optimization, rejected candidate and unresolved control slowdown,
and the [animation, skinning and grid-update follow-up](../benchmarks/history/2026-10-23-followup/README.md).
Each report stores the benchmark table in Markdown alongside immutable JSON
measurements, source patches and the dependency lockfile.

The fork's CPU benchmark is a reproducible measurement lane for collision math,
uniform-grid queries and rebuilds, animation sampling, and glTF skin deformation.
The `macroquad-cpu-v2` suite additionally covers small-scene wide grid queries,
same-cell updates, triangle surfaces, large animation clips, morphing and pose
updates. It starts a new measurement series; v1 and v2 are not directly comparable.
It measures CPU work in nanoseconds per operation. It does not measure GPU time,
rendering throughput, frame pacing, or the complete game's FPS. These require
separate future render and end-to-end lanes with their own scene and backend
fingerprints.

## Record a run

Python 3.10 or newer, Cargo, Rust, and Git are required. The runner uses only the
Python standard library; it introduces no benchmark or CI dependencies. Run
from the fork checkout:

The library does not commit a root `Cargo.lock`. On a fresh checkout, copy the
desired historical `dependencies.lock` to `Cargo.lock` to reproduce that dependency
set, or run `cargo generate-lockfile` to deliberately start a new series. The
runner requires this file and always builds with `--locked`; it never updates
dependencies silently.

```sh
python3 scripts/benchmark.py run --features gltf --label baseline
```

The default configuration uses three independent executions, nine samples per
workload, 75 ms warmup per workload, and a 30 ms target sample duration. The suite
calibrates its iteration counts independently for each workload. Calibration,
fixture construction, and compilation are outside the measured samples. The
runner builds the release benchmark with `cargo bench --locked --no-run`, then
runs that exact executable. Default-feature and `--features gltf` runs are
different suites and cannot be compared.

For less noise on Linux, select one quiet logical CPU consistently:

```sh
taskset -c 2 python3 scripts/benchmark.py run --features gltf --label baseline \
  --repeats 3 --samples 9 --warmup-ms 75 --sample-ms 30
```

Choose a CPU available in your process's affinity mask. Avoid concurrent builds,
other benchmark runs, heavy interactive applications, and changes to the
machine's power settings. Affinity is recorded on platforms supporting
`os.sched_getaffinity`; the runner does not change affinity or power settings.
WSL runs are a distinct host/backend environment from native Windows runs.

To measure a subset, use `--filter grid.`. To list the suite's cases without
timing them, run `cargo bench --bench fork_performance --features gltf -- --list`.
`--output-root PATH` selects the local evidence directory; the default is the
ignored `performance-results/` folder. `--target`, `--no-default-features`,
`--build-timeout`, and `--run-timeout` are also available. The default timeouts are
15 minutes for the build and 10 minutes per execution; no shell is used to run
commands.

`--rustc PATH` selects the compiler for both the build and the version record.
Without it, the runner uses `RUSTC` when set, otherwise `rustc` from `PATH`. It
binds the resolved executable path through `RUSTC` for Cargo and records it, so
Cargo configuration cannot silently select a different compiler from the probe.

## Retained evidence

Every run gets a fresh UTC timestamp, label, and random ID directory. Files are
created exclusively and never overwritten. A failed run is retained with
`status: failed` and its error, and is ineligible for comparison. Each directory
contains:

- `run.json`: source identity, environment, workload identities, every raw sample
  grouped by execution, median, p95, mean, standard deviation, MAD, and IQR.
- `run.md`: an automatically saved Markdown table for every complete run,
  including baselines without a comparison; links to its full JSON evidence.
- `source.zip`: exact build-relevant sources, benchmark harness, manifests, and
  fixtures, including untracked files. The `run.json` file hashes are the index.
  All crate `src/` files and literal Rust `include!`, `include_str!`, and
  `include_bytes!` inputs are included, including fonts and binary UI assets.
- `source.patch`, `Cargo.lock`: tracked changes against the recorded Git HEAD and
  the exact locked dependency resolution. The source archive also retains
  untracked source files that a Git patch alone cannot recover.
- `build.stdout.log`, `build.stderr.log`, and `repeat-NNN.stdout.log`/
  `repeat-NNN.stderr.log`: compiler output and original JSONL measurements.
- `comparison.json` and `comparison.md` when `--baseline` was supplied and the
  previous run was compatible.

Provenance includes Git HEAD and dirty status, per-file SHA-256 and an aggregate
source SHA-256, separate harness and fixture fingerprints, lockfile and manifest
fingerprints, `rustc -Vv`, Cargo and Python versions, OS/CPU/memory details,
hostname, Linux CPU affinity, compilation options, relevant compiler environment
variables, and hashes of applicable Cargo configuration files. Benchmark inputs
have independent deterministic fingerprints and output checksums. File hashes
describe bytes, so CRLF/LF differences are intentionally visible.

Raw OS-reported memory is retained. For host compatibility, reported memory is
rounded to the nearest 64 MiB and this accounting quantum is recorded explicitly.
This permits tiny Linux/WSL `MemTotal` changes caused by OS page accounting;
different memory buckets are rejected. CPU, affinity, hostname, OS, and all other
host fields still require exact matches.

The runner checks source and executable hashes before and after each execution.
Editing benchmarked source during a build or run invalidates that artifact.
Documentation, the runner's Python tests, previous measurement history, generated
build outputs, and the active output directory are excluded from source hashes.
Harness edits require a new baseline. Documentation can be updated while a run
is active.

Retain the full local evidence directories for source recovery. To maintain a
reviewable source-controlled timeline, copy the compact `run.json` records and
comparison records into `benchmarks/history/`, with a short findings entry here.
That history directory is excluded from source fingerprints and archives to
avoid recursively embedding past runs. A compact record alone is sufficient
for artifact-only timing comparisons; retain its matching full evidence directory
when exact uncommitted-source recovery is required. Do not treat these artifacts
as remote deployment backups.

## Compare an optimization

Freeze the harness and workload fixtures before measuring the baseline. Make the
production change, run meaningful correctness tests, and rerun the same command
on the same machine with the same options:

```sh
python3 scripts/benchmark.py run --features gltf --label candidate \
  --baseline performance-results/BASELINE_RUN
```

Saved artifacts can be compared later without a compiler, checkout, or benchmark
execution:

```sh
python3 scripts/benchmark.py compare benchmarks/history/BASELINE.json \
  benchmarks/history/CANDIDATE.json --threshold-percent 5
```

Compatibility is strict: schema, suite, CPU backend, workload definitions,
fixtures and output checksums, harness, manifests, lockfile, toolchain, host,
affinity, build environment, features, target, and sampling settings must match.
Production source hashes are allowed to differ; that is the change being
measured. Incompatible artifacts produce a clear error and are never combined
into a performance claim. Changing a benchmark or dependency requires recording
an additional baseline under the new configuration.

The report shows median and p95 changes across batch-average ns/op samples;
p95 is not the tail latency of individual operations. Negative changes are faster. The default
classification guard is the larger of 5% and three times observed relative
noise. Noise includes sample MAD, repeat-median MAD, and half the repeat-median
range. Changes below this guard are `within_noise`; larger positive changes are
`regression`, and larger negative changes are `improvement`. This conservative
engineering guard is not a statistical significance test. Examine the preserved
distributions, repeat the measurement on a quiet machine when uncertain, and
confirm practical effects using representative scenes before claiming an FPS
improvement.

`--threshold-percent` and `--noise-multiplier` configure the guard.
`--fail-on-regression` returns exit code 2 when a compatible comparison reports a
regression. Invalid/incompatible input returns 1. `compare --output NEW.json`
saves the comparison, refusing to overwrite an existing file.

## Validate the tooling

```sh
python3 -m unittest discover -s scripts -p test_benchmark.py -v
```

These tests exercise malformed measurements, compatibility refusals, noisy
distributions, source-change detection, untracked-source and fixture provenance,
source recovery archives, output immutability, artifact-only comparison, and
timeout log retention. They assert correctness boundaries rather than machine
speed.
