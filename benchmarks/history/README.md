# Recorded performance

Each dated directory retains immutable run records (including raw timing samples,
source/fixture hashes and environment), comparisons, the measured engine patch and
the dependency lockfile. Full source archives and build logs stay with the local
round evidence. Read [the guide](../../docs/performance.md) before comparing runs.

This is a CPU microbenchmark history. Numbers from different machines, toolchains,
build settings or workload versions are separate series, and these timings do not
measure game frame rate, rendering, browser performance or process memory.

Append new records; never replace an earlier result or discard a failed experiment.

- [23 October 2026: baseline, rejected HashSet trial, bounded grid optimization and control diagnostics](2026-10-23/README.md).
- [23 October 2026: animation, skinning and grid-update optimization](2026-10-23-followup/README.md).
