#!/usr/bin/env python3
"""Meaningful boundary/provenance tests; no dependencies or timing assertions."""

import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch
import zipfile

SPEC = importlib.util.spec_from_file_location("benchmark", Path(__file__).with_name("benchmark.py"))
bench = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(bench)


def measurement(samples=(10, 10, 10), **overrides):
    return {
        "event": "measurement", "schema_version": 1,
        "suite_version": "macroquad-cpu-v1", "backend": "cpu", "benchmark": "grid.sparse",
        "workload": "stable-workload-v1", "unit": "ns_per_op", "samples_ns": list(samples),
        "iterations": 100, "operations_per_iteration": 32,
        "fixture_fingerprint": "deadbeef", "checksum_per_iteration": "01234567",
        "hits_per_iteration": 5, **overrides,
    }


def header():
    return {"event": "suite", "schema_version": 1, "suite_version": "macroquad-cpu-v1", "backend": "cpu", "samples": 3, "warmup_ms": 1, "sample_ms": 1}


def artifact(values=(10, 10, 10)):
    measurements = bench.aggregate([[measurement(values)], [measurement(values)]])
    compatibility = {
        "schema_version": 1, "suite": header(), "backend": "cpu",
        "harness_sha256": "harness", "fixture_sha256": "fixtures", "manifest_sha256": "manifests",
        "lock_sha256": "lock", "host": {"cpu": "host", "cpu_affinity": [2]},
        "toolchain": {"rustc": "version"}, "build_environment": {}, "options": {"samples": 3},
        "workloads": [bench.workload_identity(measurements[0])],
    }
    return {"schema_version": 1, "run_id": "example", "status": "complete", "compatibility": compatibility, "benchmarks": measurements, "source": {"sha256": "original-source"}}


class MeasurementTests(unittest.TestCase):
    def parse(self, *records):
        return bench.parse_measurements("\n".join(json.dumps(record) for record in records), 3)

    def test_preserves_samples_and_fixture_identity(self):
        suite, records = self.parse(header(), measurement((10, 20, 30)))
        self.assertEqual(records[0]["samples_ns"], [10, 20, 30])
        self.assertEqual(suite["backend"], "cpu")
        self.assertEqual(records[0]["fixture_fingerprint"], "deadbeef")

    def test_rejects_missing_header_and_duplicate_benchmark(self):
        for records in ((measurement(),), (header(), measurement(), measurement()), (header(), header(), measurement())):
            with self.subTest(records=records), self.assertRaises(bench.BenchmarkError):
                self.parse(*records)

    def test_rejects_invalid_samples_and_iteration_counts(self):
        bad = [measurement(samples_ns=[0, 1, 2]), measurement(samples_ns=[-1, 1, 2]), measurement(samples_ns=[float("nan"), 1, 2]), measurement(samples_ns=[True, 1, 2]), measurement(samples_ns=[1, 2]), measurement(iterations=True), measurement(operations_per_iteration=1.5), measurement(hits_per_iteration=-1)]
        for record in bad:
            with self.subTest(record=record), self.assertRaises(bench.BenchmarkError):
                self.parse(header(), record)

    def test_rejects_non_object_json_and_unrecognized_event(self):
        for record in ([], 1, {"event": "surprise"}):
            with self.subTest(record=record), self.assertRaises(bench.BenchmarkError):
                self.parse(record)

    def test_statistics_include_tail_and_dispersion(self):
        summary = bench.summarize([10, 20, 30, 40])
        self.assertEqual(summary["median_ns"], 25)
        self.assertAlmostEqual(summary["p95_ns"], 38.5)
        self.assertEqual(summary["mad_ns"], 10)
        self.assertEqual(summary["iqr_ns"], 15)

    def test_repeats_must_have_same_inputs_and_outputs(self):
        for field, value in (("fixture_fingerprint", "other"), ("checksum_per_iteration", "different"), ("hits_per_iteration", 99)):
            changed = measurement(**{field: value})
            with self.subTest(field=field), self.assertRaises(bench.BenchmarkError):
                bench.aggregate([[measurement()], [changed]])


class ComparisonTests(unittest.TestCase):
    def test_small_os_memory_accounting_changes_do_not_split_same_host(self):
        first = {"system": "Linux", "memory_bytes": 63195701248, "cpu_affinity": [2]}
        second = {**first, "memory_bytes": 63195693056}
        self.assertEqual(bench.compatible_host(first), bench.compatible_host(second))
        self.assertEqual(first["memory_bytes"], 63195701248)
        self.assertEqual(bench.compatible_host(first)["memory_accounting_quantum_bytes"], 64 * 1024 * 1024)
        self.assertNotEqual(bench.compatible_host(first), bench.compatible_host({**first, "memory_bytes": 32 * 1024 ** 3}))

    def test_real_source_optimization_is_comparable(self):
        before, after = artifact(), artifact((8, 8, 8))
        after["source"]["sha256"] = "optimized-source"
        result = bench.compare_runs(before, after)
        self.assertEqual(result["benchmarks"][0]["status"], "improvement")
        self.assertAlmostEqual(result["benchmarks"][0]["median_change_percent"], -20)

    def test_host_toolchain_dependency_workload_and_harness_mismatch_refused(self):
        before = artifact()
        for key in before["compatibility"]:
            after = copy.deepcopy(before)
            after["compatibility"][key] = "changed"
            with self.subTest(field=key), self.assertRaisesRegex(bench.BenchmarkError, key):
                bench.compare_runs(before, after)

    def test_noise_guard_avoids_reporting_noisy_change_as_improvement(self):
        before, after = artifact((10, 10, 10)), artifact((6, 8, 12))
        result = bench.compare_runs(before, after)
        self.assertEqual(result["benchmarks"][0]["status"], "within_noise")
        self.assertGreater(result["benchmarks"][0]["effective_threshold_percent"], 20)

    def test_repeat_drift_is_also_noise(self):
        before, after = artifact(), artifact()
        after["benchmarks"] = bench.aggregate([[measurement((6, 6, 6))], [measurement((10, 10, 10))]])
        result = bench.compare_runs(before, after)
        self.assertEqual(result["benchmarks"][0]["status"], "within_noise")

    def test_regression_threshold_and_sample_reconstruction(self):
        before, after = artifact(), artifact((12, 12, 12))
        after["benchmarks"][0]["statistics"]["median_ns"] = 1
        after["benchmarks"][0]["repeat_medians_ns"] = [1, 1]
        result = bench.compare_runs(before, after)
        self.assertTrue(result["has_regression"])
        self.assertEqual(result["benchmarks"][0]["candidate"]["median_ns"], 12)

    def test_artifact_samples_cannot_diverge_from_retained_repeats(self):
        before, after = artifact(), artifact()
        after["benchmarks"][0]["samples_ns"][0] = 99
        with self.assertRaisesRegex(bench.BenchmarkError, "original repeats"):
            bench.compare_runs(before, after)

    def test_different_actual_workload_refused_even_if_metadata_claims_same(self):
        before, after = artifact(), artifact()
        after["benchmarks"][0]["fixture_fingerprint"] = "different"
        with self.assertRaisesRegex(bench.BenchmarkError, "workload/fixture/output"):
            bench.compare_runs(before, after)

    def test_artifact_only_compare_does_not_call_build(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            bench.write_json(root / "before.json", artifact())
            bench.write_json(root / "after.json", artifact((8, 8, 8)))
            with patch.object(bench, "captured_command", side_effect=AssertionError("must not run")), patch("builtins.print"):
                status = bench.main(["compare", str(root / "before.json"), str(root / "after.json"), "--output", str(root / "comparison.json")])
            self.assertEqual(status, 0)
            self.assertTrue((root / "comparison.json").is_file())


class ProvenanceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        subprocess.run(["git", "init", "--quiet", str(self.root)], check=True, capture_output=True)
        self.write(".gitignore", "target/\nCargo.lock\n")
        self.write("Cargo.toml", '[package]\nname = "example"\nversion = "0.1.0"\n')
        self.write("Cargo.lock", "version = 3\n")
        self.write("src/lib.rs", "pub fn original() {}\n")
        self.write("benches/fork_performance.rs", "fn main() {}\n")
        self.write("tests/fixtures/input.bin", b"\x00\x01\x02")
        self.write("scripts/benchmark.py", "# harness\n")

    def tearDown(self):
        self.temp.cleanup()

    def write(self, name, data):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data if isinstance(data, bytes) else data.encode())

    def test_untracked_source_lock_and_binary_fixtures_are_fingerprinted(self):
        snapshot = bench.source_snapshot(self.root)
        self.assertIn("src/lib.rs", snapshot["files"])
        self.assertIn("Cargo.lock", snapshot["files"])
        self.assertIn("tests/fixtures/input.bin", snapshot["fixture_files"])
        self.assertTrue(snapshot["lock_sha256"])

    def test_every_crate_src_binary_and_literal_include_is_archived(self):
        self.write("src/embedded.ttf", b"font")
        self.write("subcrate/src/skin.img", b"skin")
        self.write(".gitignore", "target/\nCargo.lock\nembedded.dat\n")
        self.write("embedded.dat", b"ignored but compiled")
        self.write("other/input.bin", b"included raw literal")
        self.write("other/recursive.rs", 'const DATA: &[u8] = include_bytes!("input.bin");\n')
        self.write("examples/consumer.rs", 'const DATA: &[u8] = include_bytes!("../embedded.dat");\ninclude!(r#"../other/recursive.rs"#);\n')
        snapshot = bench.source_snapshot(self.root)
        for name in ("src/embedded.ttf", "subcrate/src/skin.img", "embedded.dat", "other/input.bin"):
            self.assertIn(name, snapshot["files"])
        self.write("src/embedded.ttf", b"changed font")
        with self.assertRaisesRegex(bench.BenchmarkError, "embedded.ttf"):
            bench.ensure_unchanged(snapshot, self.root)

    def test_external_literal_include_refused_instead_of_incomplete_snapshot(self):
        self.write("src/lib.rs", 'const DATA: &[u8] = include_bytes!("../../outside-repo.dat");')
        with self.assertRaisesRegex(bench.BenchmarkError, "outside the repository"):
            bench.source_snapshot(self.root)

    def test_docs_tests_results_and_target_do_not_pollute_source_snapshot(self):
        before = bench.source_snapshot(self.root)
        self.write("docs/performance.md", "Notes\n")
        self.write("scripts/test_benchmark.py", "# tests\n")
        self.write("benchmarks/history/run.json", '{"result": 42}')
        self.write("performance-results/run/run.json", '{"result": 42}')
        self.write("target/generated.rs", "generated")
        self.assertEqual(before, bench.source_snapshot(self.root))

    def test_custom_output_root_is_excluded(self):
        before = bench.source_snapshot(self.root)
        self.write("custom-results/run.json", "{}")
        with patch.object(bench, "ACTIVE_OUTPUT_ROOT", self.root / "custom-results"):
            self.assertEqual(before, bench.source_snapshot(self.root))

    def test_production_changes_preserve_harness_and_dependency_identity(self):
        before = bench.source_snapshot(self.root)
        self.write("src/lib.rs", "pub fn optimized() {}\n")
        after = bench.source_snapshot(self.root)
        self.assertNotEqual(before["sha256"], after["sha256"])
        for key in ("harness_sha256", "fixture_sha256", "manifest_sha256", "lock_sha256"):
            self.assertEqual(before[key], after[key])
        with self.assertRaisesRegex(bench.BenchmarkError, "src/lib.rs"):
            bench.ensure_unchanged(before, self.root)

    def test_new_file_during_measurement_is_detected(self):
        before = bench.source_snapshot(self.root)
        self.write("src/new_untracked.rs", "pub fn new_code() {}")
        with self.assertRaisesRegex(bench.BenchmarkError, "new_untracked"):
            bench.ensure_unchanged(before, self.root)

    def test_source_archive_preserves_exact_fixture_and_source_bytes(self):
        destination = self.root / "performance-results" / "archive"
        destination.mkdir(parents=True)
        snapshot = bench.source_snapshot(self.root)
        # Empty repos have no HEAD; isolate only the informational diff command.
        with patch.object(bench, "git", return_value="source patch"):
            bench.save_source(destination, snapshot, self.root)
        with zipfile.ZipFile(destination / "source.zip") as archive:
            for name, sha256 in snapshot["files"].items():
                if sha256 is not None:
                    self.assertEqual(bench.digest(archive.read(name)), sha256)
        self.assertEqual((destination / "Cargo.lock").read_bytes(), (self.root / "Cargo.lock").read_bytes())

    def test_existing_artifact_is_never_overwritten(self):
        path = self.root / "result.json"
        bench.write_json(path, {"original": True})
        with self.assertRaises(FileExistsError):
            bench.write_json(path, {"original": False})
        self.assertEqual(json.loads(path.read_text()), {"original": True})

    def test_failed_or_incomplete_artifacts_cannot_be_loaded(self):
        for index, value in enumerate(({"schema_version": 1, "status": "failed"}, {"schema_version": 1, "status": "complete", "compatibility": {}, "benchmarks": [{}]})):
            path = self.root / f"invalid-{index}.json"
            bench.write_json(path, value)
            with self.assertRaises(bench.BenchmarkError):
                bench.load_run(path)

    def test_timeout_keeps_process_output(self):
        with self.assertRaisesRegex(bench.BenchmarkError, "failed"):
            bench.captured_command([sys.executable, "-c", "import time; print('started', flush=True); time.sleep(10)"], self.root, "slow", 0.2)
        self.assertIn("started", (self.root / "slow.stdout.log").read_text())
        self.assertTrue((self.root / "slow.stderr.log").exists())


class MarkdownReportTests(unittest.TestCase):
    def record(self):
        record = artifact((10, 20, 30))
        record.update({
            "options": {"features": ["gltf"], "target": "host", "no_default_features": False,
                        "repeats": 2, "samples": 3, "warmup_ms": 75, "sample_ms": 30},
            "host": {"hostname": "host|name", "system": "Linux", "release": "test", "cpu_models": ["CPU"]},
            "git": {"head": "example-commit"},
        })
        return record

    def test_summary_uses_original_samples_and_escapes_table_metadata(self):
        record = self.record()
        record["benchmarks"][0]["statistics"]["median_ns"] = 999
        report = bench.run_text(record)
        self.assertIn("| grid.sparse | 20.00 | 30.00 | 50.00% | 6 |", report)
        self.assertIn("host\\|name", report)
        self.assertIn("(run.json)", report)
        self.assertIn("do not measure", report)
        record["benchmarks"][0]["samples_ns"][0] = 99
        with self.assertRaisesRegex(bench.BenchmarkError, "aggregate samples differ"):
            bench.run_text(record)

    def test_markdown_report_never_overwrites_existing_file(self):
        with tempfile.TemporaryDirectory() as directory:
            record = self.record()
            bench.write_run_markdown(directory, record)
            path = Path(directory) / "run.md"
            original = path.read_bytes()
            record["run_id"] = "different"
            with self.assertRaises(FileExistsError):
                bench.write_run_markdown(directory, record)
            self.assertEqual(path.read_bytes(), original)

    def test_windows_cpu_metadata_is_rendered_without_linux_proc_fields(self):
        record = self.record()
        record["host"] = {"hostname": "desktop", "system": "Windows", "release": "11",
                          "hardware": {"cpus": [{"Name": "CPU A"}, {"Name": "CPU B"}]},
                          "processor": "generic identifier"}
        self.assertIn("Windows 11; CPU A, CPU B.", bench.run_text(record))

    def test_missing_or_empty_cpu_probes_fall_back_without_losing_the_report(self):
        for fields, expected in [
            ({"cpu_models": [], "processor": "portable CPU"}, "portable CPU"),
            ({"hardware": {"cpus": []}, "processor_identifier": "fallback CPU"}, "fallback CPU"),
            ({"processor": "", "machine": "arm64"}, "arm64"),
            ({}, "unknown CPU"),
        ]:
            with self.subTest(fields=fields):
                record = self.record()
                record["host"] = {"hostname": "host", "system": "Other", "release": "test", **fields}
                self.assertIn(f"Other test; {expected}.", bench.run_text(record))


class CompilerBindingTests(unittest.TestCase):
    def test_explicit_compiler_is_inherited_by_child_commands_and_restored(self):
        args = SimpleNamespace(rustc=sys.executable)
        original = "original-compiler"
        expected = os.path.abspath(sys.executable)

        def observe(arguments, compiler):
            self.assertIs(arguments, args)
            self.assertEqual(compiler, expected)
            self.assertEqual(os.environ["RUSTC"], expected)
            observed = bench.command([sys.executable, "-c", "import os; print(os.environ['RUSTC'])"])
            self.assertEqual(observed, expected)
            self.assertEqual(bench.build_environment()["environment"]["RUSTC"], expected)
            return 0

        with patch.dict(os.environ, {"RUSTC": original}), patch.object(bench, "run_with_compiler", side_effect=observe):
            self.assertEqual(bench.run_benchmark(args), 0)
            self.assertEqual(os.environ["RUSTC"], original)

    def test_default_uses_inherited_rustc_and_restores_after_failure(self):
        with patch.dict(os.environ, {"RUSTC": sys.executable}), patch.object(bench, "run_with_compiler", side_effect=bench.BenchmarkError("failed")) as invoke:
            with self.assertRaisesRegex(bench.BenchmarkError, "failed"):
                bench.run_benchmark(SimpleNamespace(rustc=None))
            self.assertEqual(invoke.call_args.args[1], os.path.abspath(sys.executable))
            self.assertEqual(os.environ["RUSTC"], sys.executable)

    def test_missing_compiler_is_rejected_before_artifact_creation(self):
        with patch.object(bench.shutil, "which", return_value=None), self.assertRaisesRegex(bench.BenchmarkError, "Cannot find Rust compiler"):
            bench.run_benchmark(SimpleNamespace(rustc="missing-rustc"))


if __name__ == "__main__":
    unittest.main()
