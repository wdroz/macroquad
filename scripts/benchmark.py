#!/usr/bin/env python3
"""Run the CPU benchmark and retain reproducible, comparable local evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import shutil
import statistics
import subprocess
import sys
import time
import uuid
import zipfile
from datetime import datetime, timezone

SCHEMA_VERSION = 1
BENCH = "fork_performance"
REPO = Path(__file__).resolve().parents[1]
SOURCE_SUFFIXES = {
    ".rs", ".toml", ".lock", ".wgsl", ".glsl", ".vert", ".frag", ".geom",
    ".h", ".c", ".cc", ".cpp", ".s", ".inc", ".json", ".gltf", ".glb",
}
EXCLUDED_DIRS = {".git", "target", "performance-results", "artifacts", "backups", "output"}
ACTIVE_OUTPUT_ROOT = None
MEMORY_ACCOUNTING_QUANTUM = 64 * 1024 * 1024
INCLUDE_LITERAL = re.compile(r'\binclude(?:_bytes|_str)?!\s*\(\s*(?:"(?P<normal>(?:\\.|[^"\\])*)"|r(?P<hashes>#*)"(?P<raw>.*?)"(?P=hashes))', re.DOTALL)


class BenchmarkError(RuntimeError):
    pass


def digest(data):
    return hashlib.sha256(data).hexdigest()


def stable_hash(value):
    return digest(json.dumps(value, sort_keys=True, separators=(",", ":")).encode())


def write_json(path, value):
    # Exclusive creation is intentional: historical evidence must never be replaced.
    with Path(path).open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(value, stream, indent=2, sort_keys=True, allow_nan=False)
        stream.write("\n")


def command(argv, cwd=REPO, timeout=60):
    try:
        result = subprocess.run(argv, cwd=cwd, capture_output=True, timeout=timeout)
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise BenchmarkError(f"Command failed: {argv!r}: {exc}") from exc
    if result.returncode:
        raise BenchmarkError(
            f"Command exited {result.returncode}: {argv!r}\n"
            + result.stderr.decode("utf-8", errors="replace")
        )
    return result.stdout.decode("utf-8", errors="replace").strip()


def git(*args, repo=REPO):
    # WSL may see a Windows-owned checkout. Trust only this explicitly selected repo.
    return command(["git", "-c", f"safe.directory={repo.as_posix()}", *args], cwd=repo)


def source_paths(repo=REPO):
    names = set(git("ls-files", "--cached", "--others", "--exclude-standard", "-z", repo=repo).split("\0"))
    names.add("Cargo.lock")  # Cargo.lock is deliberately ignored in this library checkout.
    names.add("scripts/benchmark.py")
    selected = []
    for name in sorted(names):
        relative = Path(name)
        if not name or relative.is_absolute() or ".." in relative.parts:
            continue
        if any(part in EXCLUDED_DIRS for part in relative.parts):
            continue
        if relative.parts[:2] == ("benchmarks", "history"):
            continue
        if ACTIVE_OUTPUT_ROOT is not None and (repo / relative).resolve().is_relative_to(ACTIVE_OUTPUT_ROOT):
            continue
        fixture = any("fixture" in part.lower() or part == "assets" for part in relative.parts)
        if relative.suffix.lower() in SOURCE_SUFFIXES or "src" in relative.parts or fixture or name.startswith("benches/") or name == "scripts/benchmark.py" or relative.name.startswith("rust-toolchain"):
            selected.append(name)
    # Rust may embed arbitrary binary/text files outside a crate's src tree.
    # Follow literal includes even when the included file is otherwise ignored
    # by Git; suffix-based discovery alone misses fonts and binary UI skins.
    selected = set(selected)
    pending = [name for name in selected if name.endswith(".rs")]
    examined = set()
    while pending:
        name = pending.pop()
        if name in examined or not (repo / name).is_file():
            continue
        examined.add(name)
        source = (repo / name).read_text(encoding="utf-8")
        source = re.sub(r"(?m)^\s*//.*$", "", source)
        for match in INCLUDE_LITERAL.finditer(source):
            literal = json.loads('"' + match["normal"] + '"') if match["normal"] is not None else match["raw"]
            path = Path(os.path.abspath((repo / name).parent / literal))
            if not path.is_relative_to(repo):
                raise BenchmarkError(f"Cannot archive include outside the repository: {name}: {literal}")
            relative = path.relative_to(repo).as_posix()
            if path.is_file() or relative in names:
                selected.add(relative)
                if relative.endswith(".rs"):
                    pending.append(relative)
    return sorted(selected)


def source_snapshot(repo=REPO):
    hashes = {}
    for name in source_paths(repo):
        path = repo / name
        if path.is_symlink():
            # A source archive must not silently omit code resolved outside the checkout.
            raise BenchmarkError(f"Cannot snapshot symlinked source: {name}")
        hashes[name] = digest(path.read_bytes()) if path.is_file() else None
    fixtures = {name: value for name, value in hashes.items() if any("fixture" in part.lower() or part == "assets" for part in Path(name).parts)}
    harness = {name: value for name, value in hashes.items() if name.startswith("benches/") or name == "scripts/benchmark.py"}
    manifests = {name: value for name, value in hashes.items() if Path(name).name == "Cargo.toml"}
    return {
        "sha256": stable_hash(hashes), "files": hashes,
        "fixture_files": fixtures, "fixture_sha256": stable_hash(fixtures),
        "harness_sha256": stable_hash(harness), "manifest_sha256": stable_hash(manifests),
        "lock_sha256": hashes.get("Cargo.lock"),
    }


def ensure_unchanged(expected, repo=REPO):
    actual = source_snapshot(repo)
    if expected["sha256"] != actual["sha256"]:
        names = sorted(set(expected["files"]) | set(actual["files"]))
        changed = [name for name in names if expected["files"].get(name) != actual["files"].get(name)]
        raise BenchmarkError("Source changed during build/run; results invalid: " + ", ".join(changed))


def save_source(directory, snapshot, repo=REPO):
    with zipfile.ZipFile(directory / "source.zip", "x", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, expected in snapshot["files"].items():
            if expected is not None:
                data = (repo / name).read_bytes()
                if digest(data) != expected:
                    raise BenchmarkError(f"Source changed while archiving: {name}")
                archive.writestr(name, data)
    if snapshot["lock_sha256"] is not None:
        with (directory / "Cargo.lock").open("xb") as stream:
            stream.write((repo / "Cargo.lock").read_bytes())
    with (directory / "source.patch").open("x", encoding="utf-8", newline="\n") as stream:
        stream.write(git("diff", "HEAD", "--binary", "--no-ext-diff", repo=repo) + "\n")


def host_metadata():
    info = {
        "system": platform.system(), "release": platform.release(),
        "version": platform.version(), "machine": platform.machine(),
        "hostname": platform.node(), "processor": platform.processor(),
        "logical_cpus": os.cpu_count(),
    }
    if hasattr(os, "sched_getaffinity"):
        info["cpu_affinity"] = sorted(os.sched_getaffinity(0))
    if sys.platform.startswith("linux"):
        cpuinfo = Path("/proc/cpuinfo")
        if cpuinfo.exists():
            models = {line.split(":", 1)[1].strip() for line in cpuinfo.read_text().splitlines() if line.startswith("model name")}
            info["cpu_models"] = sorted(models)
        meminfo = Path("/proc/meminfo")
        if meminfo.exists():
            match = re.search(r"^MemTotal:\s+(\d+) kB$", meminfo.read_text(), re.MULTILINE)
            if match:
                info["memory_bytes"] = int(match.group(1)) * 1024
    elif sys.platform == "win32":
        info["processor_identifier"] = os.environ.get("PROCESSOR_IDENTIFIER", "")
        try:
            info["hardware"] = json.loads(command([
                "powershell", "-NoProfile", "-NonInteractive", "-Command",
                "$cpus = @(Get-CimInstance Win32_Processor | Select-Object Name,Manufacturer,NumberOfCores,NumberOfLogicalProcessors,MaxClockSpeed); "
                "$memory = (Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory; "
                "@{cpus=$cpus;memory_bytes=$memory} | ConvertTo-Json -Depth 4 -Compress",
            ], timeout=30))
        except BenchmarkError as exc:
            info["hardware_probe_error"] = str(exc)
    return info


def build_environment(repo=REPO):
    prefixes = ("CARGO_PROFILE_", "CARGO_TARGET_", "CARGO_BUILD_", "RUSTUP_", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")
    exact = {"CC", "CXX", "AR", "LD", "CFLAGS", "CXXFLAGS", "LDFLAGS", "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "CARGO_HOME"}
    env = {key: value for key, value in os.environ.items() if key in exact or key.startswith(prefixes)}
    configs = {}
    # Cargo configuration can affect code generation outside the repository.
    locations = [parent / ".cargo" for parent in [repo, *repo.parents]]
    locations.append(Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo"))))
    for directory in locations:
        for filename in ("config", "config.toml"):
            path = directory / filename
            if path.is_file():
                configs[str(path.resolve())] = digest(path.read_bytes())
    return {"environment": env, "cargo_config_sha256": configs}


def compatible_host(host):
    # Linux/WSL can adjust /proc/meminfo MemTotal by a few pages between runs.
    # Preserve the raw measurement above, but compare the explicit 64 MiB bucket.
    result = dict(host)
    if "memory_bytes" in result:
        memory = result.pop("memory_bytes")
        result["memory_accounting_quantum_bytes"] = MEMORY_ACCOUNTING_QUANTUM
        result["memory_bytes_rounded"] = ((memory + MEMORY_ACCOUNTING_QUANTUM // 2) // MEMORY_ACCOUNTING_QUANTUM) * MEMORY_ACCOUNTING_QUANTUM
    return result


def percentile(values, fraction):
    ordered = sorted(values)
    index = (len(ordered) - 1) * fraction
    low = math.floor(index)
    high = math.ceil(index)
    return ordered[low] + (ordered[high] - ordered[low]) * (index - low)


def summarize(values):
    if not values or any(isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value <= 0 for value in values):
        raise BenchmarkError("Timing samples must be finite positive numbers")
    median = statistics.median(values)
    mad = statistics.median(abs(value - median) for value in values)
    return {
        "count": len(values), "median_ns": median, "p95_ns": percentile(values, 0.95),
        "min_ns": min(values), "max_ns": max(values), "mean_ns": statistics.mean(values),
        "stdev_ns": statistics.stdev(values) if len(values) > 1 else 0,
        "mad_ns": mad, "relative_mad_percent": mad / median * 100,
        "iqr_ns": percentile(values, 0.75) - percentile(values, 0.25),
    }


def parse_measurements(raw, samples):
    header = None
    measurements = []
    names = set()
    for line in raw.splitlines():
        if not line.strip():
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError as exc:
            raise BenchmarkError(f"Benchmark emitted non-JSON output: {line[:200]}") from exc
        if not isinstance(event, dict):
            raise BenchmarkError("Benchmark events must be JSON objects")
        if event.get("event") == "suite":
            if header is not None:
                raise BenchmarkError("Benchmark emitted multiple suite headers")
            header = event
        elif event.get("event") == "measurement":
            if header is None:
                raise BenchmarkError("Benchmark measurement precedes suite header")
            required = {"benchmark", "workload", "unit", "samples_ns", "iterations", "operations_per_iteration", "fixture_fingerprint", "checksum_per_iteration", "hits_per_iteration", "schema_version", "suite_version", "backend"}
            if not required.issubset(event):
                raise BenchmarkError("Measurement lacks fields: " + ", ".join(sorted(required - set(event))))
            if event["benchmark"] in names:
                raise BenchmarkError("Duplicate benchmark name: " + event["benchmark"])
            if event["unit"] != "ns_per_op" or event["backend"] != "cpu" or event["schema_version"] != SCHEMA_VERSION:
                raise BenchmarkError("Unsupported benchmark unit/backend/schema")
            if not isinstance(event["samples_ns"], list) or len(event["samples_ns"]) != samples:
                raise BenchmarkError("Benchmark sample count differs from request")
            summarize(event["samples_ns"])
            if any(type(event[key]) is not int or event[key] <= 0 for key in ("iterations", "operations_per_iteration")):
                raise BenchmarkError("Benchmark iteration counts must be positive")
            if type(event["hits_per_iteration"]) is not int or event["hits_per_iteration"] < 0:
                raise BenchmarkError("Benchmark hit count must be a nonnegative integer")
            names.add(event["benchmark"])
            measurements.append(event)
        else:
            raise BenchmarkError(f"Unexpected benchmark event: {event.get('event')!r}")
    if not header or not measurements:
        raise BenchmarkError("Benchmark emitted no suite header or measurements")
    if header.get("schema_version") != SCHEMA_VERSION or header.get("backend") != "cpu":
        raise BenchmarkError("Unsupported suite schema/backend")
    if any(item["suite_version"] != header.get("suite_version") for item in measurements):
        raise BenchmarkError("Measurement suite differs from header")
    return header, measurements


def workload_identity(measurement):
    keys = ("benchmark", "workload", "unit", "operations_per_iteration", "fixture_fingerprint", "checksum_per_iteration", "hits_per_iteration")
    return {key: measurement[key] for key in keys}


def aggregate(repeats):
    first = repeats[0]
    expected = {item["benchmark"]: workload_identity(item) for item in first}
    grouped = {name: [] for name in expected}
    for repeat in repeats:
        current = {item["benchmark"]: workload_identity(item) for item in repeat}
        if current != expected:
            raise BenchmarkError("Workloads, fixtures, or output checksums changed between repeats")
        for item in repeat:
            grouped[item["benchmark"]].append(item)
    result = []
    for name, items in grouped.items():
        samples = [value for item in items for value in item["samples_ns"]]
        repeat_medians = [statistics.median(item["samples_ns"]) for item in items]
        result.append({
            **expected[name], "samples_ns": samples, "statistics": summarize(samples),
            "repeat_medians_ns": repeat_medians,
            "repeat_statistics": summarize(repeat_medians),
            "repeats": items,
        })
    return result


def load_run(path):
    path = Path(path)
    if path.is_dir():
        path /= "run.json"
    try:
        run = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise BenchmarkError(f"Cannot read benchmark artifact {path}: {exc}") from exc
    if run.get("schema_version") != SCHEMA_VERSION or run.get("status") != "complete":
        raise BenchmarkError(f"Not a complete supported benchmark artifact: {path}")
    required_compatibility = {"schema_version", "suite", "backend", "harness_sha256", "fixture_sha256", "manifest_sha256", "lock_sha256", "host", "toolchain", "build_environment", "options", "workloads"}
    if not isinstance(run.get("compatibility"), dict) or not required_compatibility.issubset(run["compatibility"]) or not run.get("benchmarks"):
        raise BenchmarkError(f"Benchmark artifact lacks compatibility or measurements: {path}")
    return run


def repeat_medians(benchmark):
    repeats = benchmark.get("repeats", [])
    if not repeats:
        raise BenchmarkError("Artifact lacks original repeat measurements")
    all_samples = [value for repeat in repeats for value in repeat["samples_ns"]]
    if all_samples != benchmark["samples_ns"]:
        raise BenchmarkError("Artifact aggregate samples differ from original repeats")
    for repeat in repeats:
        summarize(repeat["samples_ns"])
        if workload_identity(repeat) != workload_identity(benchmark):
            raise BenchmarkError("Artifact workload identity differs between repeats")
    return [statistics.median(repeat["samples_ns"]) for repeat in repeats]


def compare_runs(baseline, candidate, threshold=5.0, noise_multiplier=3.0):
    old = baseline["compatibility"]
    new = candidate["compatibility"]
    differences = [key for key in sorted(set(old) | set(new)) if old.get(key) != new.get(key)]
    if differences:
        raise BenchmarkError("Incompatible benchmark artifacts; refusing comparison. Differing fields: " + ", ".join(differences))
    before = {item["benchmark"]: item for item in baseline["benchmarks"]}
    after = {item["benchmark"]: item for item in candidate["benchmarks"]}
    if before.keys() != after.keys():
        raise BenchmarkError("Incompatible benchmark sets; refusing comparison")
    comparisons = []
    for name in before:
        left, right = before[name], after[name]
        if workload_identity(left) != workload_identity(right):
            raise BenchmarkError(f"Incompatible workload/fixture/output for {name}; refusing comparison")
        # Recompute summaries: an artifact's stored aggregate is never trusted over samples.
        a, b = summarize(left["samples_ns"]), summarize(right["samples_ns"])
        ar = summarize(repeat_medians(left))
        br = summarize(repeat_medians(right))
        delta = (b["median_ns"] / a["median_ns"] - 1) * 100
        # MAD catches sample dispersion; repeat-median range also captures noisy runs
        # with very few repeats. This is an engineering noise guard, not a CI test.
        noise = max(a["relative_mad_percent"], b["relative_mad_percent"], ar["relative_mad_percent"], br["relative_mad_percent"],
                    (ar["max_ns"] - ar["min_ns"]) / ar["median_ns"] * 50,
                    (br["max_ns"] - br["min_ns"]) / br["median_ns"] * 50)
        guard = max(threshold, noise_multiplier * noise)
        status = "regression" if delta > guard else "improvement" if delta < -guard else "within_noise"
        comparisons.append({
            "benchmark": name, "status": status, "median_change_percent": delta,
            "p95_change_percent": (b["p95_ns"] / a["p95_ns"] - 1) * 100,
            "baseline": a, "candidate": b, "noise_percent": noise,
            "effective_threshold_percent": guard,
        })
    return {
        "schema_version": SCHEMA_VERSION,
        "baseline_run_id": baseline["run_id"], "candidate_run_id": candidate["run_id"],
        "threshold_percent": threshold, "noise_multiplier": noise_multiplier,
        "benchmarks": comparisons,
        "has_regression": any(item["status"] == "regression" for item in comparisons),
    }


def comparison_text(comparison):
    lines = ["Benchmark comparison (negative change is faster)", "", "| Benchmark | Baseline median ns/op | Candidate median ns/op | Median change | p95 change | Noise guard | Result |", "|---|---:|---:|---:|---:|---:|---|"]
    for item in comparison["benchmarks"]:
        lines.append(f"| {item['benchmark']} | {item['baseline']['median_ns']:.2f} | {item['candidate']['median_ns']:.2f} | {item['median_change_percent']:+.2f}% | {item['p95_change_percent']:+.2f}% | {item['effective_threshold_percent']:.2f}% | {item['status']} |")
    lines += ["", "CPU microbenchmarks only; these measurements do not establish frame time, GPU throughput, or end-to-end game performance."]
    return "\n".join(lines) + "\n"


def markdown_value(value):
    return str(value).replace("\\", "\\\\").replace("|", "\\|").replace("`", "\\`").replace("\r", " ").replace("\n", " ").replace("<", "&lt;").replace(">", "&gt;")


def run_text(artifact):
    """Human-readable companion; raw samples and provenance remain in run.json."""
    options = artifact["options"]
    host = artifact["host"]
    lines = [
        "# CPU benchmark run", "",
        f"Run: {markdown_value(artifact['run_id'])}", "",
        f"Source commit: {markdown_value(artifact['git']['head'])}", "",
        f"Exact source SHA-256: {artifact['source']['sha256']}", "",
        f"Host: {markdown_value(host['hostname'])}; {markdown_value(host['system'])} "
        f"{markdown_value(host['release'])}; {markdown_value(', '.join(host['cpu_models']))}.", "",
        f"Features: {markdown_value(', '.join(options['features']) or '(default)')}; "
        f"target: {markdown_value(options['target'])}; default features disabled: {options['no_default_features']}.", "",
        f"Sampling: {options['repeats']} process executions, {options['samples']} samples each, "
        f"{options['warmup_ms']} ms warmup, {options['sample_ms']} ms target batch duration.", "",
        "| Benchmark | Median ns/op | p95 ns/op | MAD | Samples |", "|---|---:|---:|---:|---:|",
    ]
    for item in artifact["benchmarks"]:
        repeat_medians(item)  # Verify originals before presenting derived summaries.
        stats = summarize(item["samples_ns"])
        lines.append(f"| {markdown_value(item['benchmark'])} | {stats['median_ns']:.2f} | {stats['p95_ns']:.2f} | {stats['relative_mad_percent']:.2f}% | {stats['count']} |")
    lines += ["", "These are batch-average CPU timings; p95 is across those averages. "
              "They do not measure individual-operation tail latency, GPU time, memory use or game FPS.", "",
              "[Raw samples and complete provenance](run.json) include fixture/output checksums, "
              "compiler, affinity, build options and exact source-file hashes. "
              "The source archive and lockfile accompany this report. Compare only compatible runs."]
    return "\n".join(lines) + "\n"


def write_run_markdown(directory, artifact):
    report = run_text(artifact)
    with (Path(directory) / "run.md").open("x", encoding="utf-8", newline="\n") as stream:
        stream.write(report)


def captured_command(argv, directory, name, timeout):
    start = time.monotonic()
    with (directory / f"{name}.stdout.log").open("xb") as stdout, (directory / f"{name}.stderr.log").open("xb") as stderr:
        try:
            result = subprocess.run(argv, cwd=REPO, stdout=stdout, stderr=stderr, timeout=timeout)
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise BenchmarkError(f"{name} failed: {exc}; retained logs in {directory}") from exc
    if result.returncode:
        raise BenchmarkError(f"{name} exited {result.returncode}; inspect {directory / (name + '.stderr.log')}")
    return time.monotonic() - start


def run_benchmark(args):
    requested = args.rustc or os.environ.get("RUSTC") or "rustc"
    located = shutil.which(requested)
    if located is None:
        raise BenchmarkError(f"Cannot find Rust compiler: {requested}")
    # Do not resolve symlinks: rustup's rustc shim dispatches by executable name.
    compiler = os.path.abspath(located)
    previous = os.environ.get("RUSTC")
    os.environ["RUSTC"] = compiler
    try:
        return run_with_compiler(args, compiler)
    finally:
        if previous is None:
            os.environ.pop("RUSTC", None)
        else:
            os.environ["RUSTC"] = previous


def run_with_compiler(args, compiler):
    global ACTIVE_OUTPUT_ROOT
    root = Path(args.output_root).resolve()
    if root == REPO or root in REPO.parents:
        raise BenchmarkError("Output root must not contain the source repository")
    ACTIVE_OUTPUT_ROOT = root
    root.mkdir(parents=True, exist_ok=True)
    now = datetime.now(timezone.utc)
    label = re.sub(r"[^A-Za-z0-9_.-]+", "-", args.label).strip("-.") or "run"
    run_id = now.strftime("%Y%m%dT%H%M%S.%fZ") + "-" + label + "-" + uuid.uuid4().hex[:8]
    directory = root / run_id
    directory.mkdir()
    print(f"Benchmark artifact: {directory}", flush=True)
    artifact = {"schema_version": SCHEMA_VERSION, "run_id": run_id, "label": args.label,
                "created_utc": now.isoformat(), "status": "running", "backend": "cpu"}
    try:
        snapshot = source_snapshot()
        if snapshot["lock_sha256"] is None:
            raise BenchmarkError("Cargo.lock is required; generate it before measuring so dependency resolution is fixed")
        artifact["source"] = snapshot
        artifact["git"] = {"head": git("rev-parse", "HEAD"), "status_porcelain": git("status", "--porcelain=v1", "--untracked-files=all"), "diff_sha256": digest(git("diff", "HEAD", "--binary", "--no-ext-diff").encode()), "repository": str(REPO)}
        artifact["toolchain"] = {"rustc_executable": compiler, "rustc_verbose": command([compiler, "-Vv"]), "cargo_version": command([args.cargo, "--version"]), "python_version": platform.python_version()}
        artifact["host"] = host_metadata()
        artifact["build_environment"] = build_environment()
        options = {
            "bench": BENCH, "profile": "bench/release", "target": args.target or "host",
            "features": sorted(set(filter(None, re.split(r"[,\s]+", args.features)))),
            "no_default_features": args.no_default_features, "samples": args.samples,
            "warmup_ms": args.warmup_ms, "sample_ms": args.sample_ms,
            "repeats": args.repeats, "filter": args.filter,
        }
        artifact["options"] = options
        save_source(directory, snapshot)
        ensure_unchanged(snapshot)
        build = [args.cargo, "bench", "--locked", "--bench", BENCH, "--no-run", "--message-format=json"]
        if options["features"]:
            build += ["--features", ",".join(options["features"])]
        if args.no_default_features:
            build.append("--no-default-features")
        if args.target:
            build += ["--target", args.target]
        artifact["build_command"] = build
        artifact["build_seconds"] = captured_command(build, directory, "build", args.build_timeout)
        ensure_unchanged(snapshot)
        executables = set()
        for line in (directory / "build.stdout.log").read_text(encoding="utf-8", errors="replace").splitlines():
            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                continue
            if event.get("reason") == "compiler-artifact" and event.get("target", {}).get("name") == BENCH and event.get("executable"):
                executables.add(event["executable"])
        if len(executables) != 1:
            raise BenchmarkError(f"Expected one compiled benchmark executable, found {sorted(executables)}")
        executable = Path(executables.pop())
        artifact["executable_sha256"] = digest(executable.read_bytes())
        argv = [str(executable), "--samples", str(args.samples), "--warmup-ms", str(args.warmup_ms), "--sample-ms", str(args.sample_ms)]
        if args.filter:
            argv += ["--filter", args.filter]
        artifact["benchmark_command"] = argv
        repeats = []
        headers = []
        durations = []
        for index in range(args.repeats):
            ensure_unchanged(snapshot)
            if digest(executable.read_bytes()) != artifact["executable_sha256"]:
                raise BenchmarkError("Benchmark executable changed during run")
            print(f"Repeat {index + 1}/{args.repeats}", flush=True)
            name = f"repeat-{index + 1:03d}"
            durations.append(captured_command(argv, directory, name, args.run_timeout))
            ensure_unchanged(snapshot)
            if digest(executable.read_bytes()) != artifact["executable_sha256"]:
                raise BenchmarkError("Benchmark executable changed during run")
            header, measurements = parse_measurements((directory / f"{name}.stdout.log").read_text(encoding="utf-8"), args.samples)
            if header.get("samples") != args.samples or header.get("warmup_ms") != args.warmup_ms or header.get("sample_ms") != args.sample_ms:
                raise BenchmarkError("Suite settings differ from requested samples/warmup/sample duration")
            if headers and header != headers[0]:
                raise BenchmarkError("Suite header changed between repeats")
            headers.append(header)
            repeats.append(measurements)
        artifact["suite"] = headers[0]
        artifact["repeat_durations_seconds"] = durations
        artifact["benchmarks"] = aggregate(repeats)
        artifact["compatibility"] = {
            "schema_version": SCHEMA_VERSION, "suite": headers[0], "backend": "cpu",
            "harness_sha256": snapshot["harness_sha256"], "fixture_sha256": snapshot["fixture_sha256"],
            "manifest_sha256": snapshot["manifest_sha256"], "lock_sha256": snapshot["lock_sha256"],
            "host": compatible_host(artifact["host"]), "toolchain": artifact["toolchain"],
            "build_environment": artifact["build_environment"], "options": options,
            "workloads": [workload_identity(item) for item in artifact["benchmarks"]],
        }
        ensure_unchanged(snapshot)
        if build_environment() != artifact["build_environment"]:
            raise BenchmarkError("Build environment or Cargo configuration changed during run")
        artifact["status"] = "complete"
        artifact["completed_utc"] = datetime.now(timezone.utc).isoformat()
        write_json(directory / "run.json", artifact)
    except Exception as exc:
        artifact["status"] = "failed"
        artifact["error"] = str(exc)
        artifact["completed_utc"] = datetime.now(timezone.utc).isoformat()
        if not (directory / "run.json").exists():
            write_json(directory / "run.json", artifact)
        raise
    write_run_markdown(directory, artifact)
    for item in artifact["benchmarks"]:
        stats = item["statistics"]
        print(f"{item['benchmark']}: median {stats['median_ns']:.2f} ns/op; p95 {stats['p95_ns']:.2f}; MAD {stats['relative_mad_percent']:.2f}%")
    if args.baseline:
        comparison = compare_runs(load_run(args.baseline), artifact, args.threshold_percent, args.noise_multiplier)
        write_json(directory / "comparison.json", comparison)
        with (directory / "comparison.md").open("x", encoding="utf-8", newline="\n") as stream:
            stream.write(comparison_text(comparison))
        print(comparison_text(comparison))
        if args.fail_on_regression and comparison["has_regression"]:
            return 2
    return 0


def positive_int(value):
    number = int(value)
    if number <= 0:
        raise argparse.ArgumentTypeError("must be greater than zero")
    return number


def nonnegative_float(value):
    number = float(value)
    if not math.isfinite(number) or number < 0:
        raise argparse.ArgumentTypeError("must be finite and nonnegative")
    return number


def bounded_int(minimum, maximum):
    def parse(value):
        number = int(value)
        if not minimum <= number <= maximum:
            raise argparse.ArgumentTypeError(f"must be between {minimum} and {maximum}")
        return number
    return parse


def make_parser():
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    run = subparsers.add_parser("run", help="build, measure, and save an immutable run")
    run.add_argument("--output-root", default=str(REPO / "performance-results"))
    run.add_argument("--label", default="run")
    run.add_argument("--repeats", type=positive_int, default=3)
    run.add_argument("--samples", type=bounded_int(3, 101), default=9)
    run.add_argument("--warmup-ms", type=bounded_int(0, 10000), default=75)
    run.add_argument("--sample-ms", type=bounded_int(1, 5000), default=30)
    run.add_argument("--features", default="")
    run.add_argument("--no-default-features", action="store_true")
    run.add_argument("--filter", default="")
    run.add_argument("--target")
    run.add_argument("--cargo", default="cargo")
    run.add_argument("--rustc", help="compiler used for both Cargo and provenance (default: RUSTC, then PATH rustc)")
    run.add_argument("--build-timeout", type=positive_int, default=900)
    run.add_argument("--run-timeout", type=positive_int, default=600)
    run.add_argument("--baseline", help="previous run directory or run.json")
    compare = subparsers.add_parser("compare", help="compare saved artifacts without compiling or running")
    compare.add_argument("baseline")
    compare.add_argument("candidate")
    compare.add_argument("--output", help="new JSON file; existing files are never overwritten")
    for target in (run, compare):
        target.add_argument("--threshold-percent", type=nonnegative_float, default=5.0)
        target.add_argument("--noise-multiplier", type=nonnegative_float, default=3.0)
        target.add_argument("--fail-on-regression", action="store_true")
    return parser


def main(argv=None):
    args = make_parser().parse_args(argv)
    try:
        if args.command == "run":
            return run_benchmark(args)
        comparison = compare_runs(load_run(args.baseline), load_run(args.candidate), args.threshold_percent, args.noise_multiplier)
        if args.output:
            write_json(args.output, comparison)
        print(comparison_text(comparison))
        return 2 if args.fail_on_regression and comparison["has_regression"] else 0
    except (BenchmarkError, OSError, ValueError, KeyError, TypeError) as exc:
        print(f"benchmark: error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
