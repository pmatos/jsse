import importlib.util
import json
import math
import os
import stat
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path
from unittest import mock


REPO_ROOT = Path(__file__).resolve().parents[1]
SCRIPTS_DIR = REPO_ROOT / "scripts"
RUNNER_PATH = SCRIPTS_DIR / "run-jetstream.py"
sys.path.insert(0, str(SCRIPTS_DIR))

from benchmark_protocol import (  # noqa: E402
    BusyHostError,
    LoadAverageUnavailable,
    choose_cpu_pinning,
    collect_host_fingerprint,
    detect_cpu_topology,
    format_cpu_list,
    read_load_averages,
    require_idle,
    summarize_repeats,
)


def load_runner_module():
    spec = importlib.util.spec_from_file_location("run_jetstream", RUNNER_PATH)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def assert_scores_are_self_consistent(case, scores):
    """Assert overall_score is the geometric mean of the sub-scores beside it.

    JetStream defines the overall score that way, so any aggregation across
    repeats has to preserve it or the published score contradicts the
    sub-scores reported next to it.
    """
    components = [scores["first_score"], scores["average_score"]]
    if scores["worst_score"] is not None:
        components.append(scores["worst_score"])
    expected = math.exp(sum(math.log(c) for c in components) / len(components))
    case.assertAlmostEqual(scores["overall_score"], expected, places=9)

    for time_key, score_key in (
        ("first_time", "first_score"),
        ("average_time", "average_score"),
        ("worst_time", "worst_score"),
    ):
        if scores[time_key] is None:
            case.assertIsNone(scores[score_key])
            continue
        case.assertAlmostEqual(
            scores[score_key], 5000.0 / max(scores[time_key], 1.0), places=9
        )


class LoadGateTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.loadavg = Path(self.tmp.name) / "loadavg"

    def tearDown(self):
        self.tmp.cleanup()

    def test_threshold_is_inclusive(self):
        self.loadavg.write_text("1.50 2.00 3.00 1/10 123\n", encoding="utf-8")

        self.assertEqual(require_idle(1.5, self.loadavg), (1.5, 2.0, 3.0))

    def test_busy_host_is_rejected(self):
        self.loadavg.write_text("1.51 2.00 3.00 1/10 123\n", encoding="utf-8")

        with self.assertRaises(BusyHostError) as raised:
            require_idle(1.5, self.loadavg)

        self.assertEqual(raised.exception.loadavg1, 1.51)
        self.assertEqual(raised.exception.threshold, 1.5)

    def test_malformed_load_average_fails_closed(self):
        self.loadavg.write_text("not-a-load-average\n", encoding="utf-8")

        with self.assertRaises(LoadAverageUnavailable):
            read_load_averages(self.loadavg)


class CpuTopologyTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.sysfs = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def write_frequency(self, cpu: int, frequency: int):
        path = self.sysfs / f"cpu{cpu}" / "cpufreq" / "cpuinfo_max_freq"
        path.parent.mkdir(parents=True)
        path.write_text(f"{frequency}\n", encoding="utf-8")

    def test_heterogeneous_topology_pins_complete_fast_cluster(self):
        for cpu, frequency in enumerate([5_000_000, 3_000_000, 3_000_000, 5_000_000]):
            self.write_frequency(cpu, frequency)

        topology = detect_cpu_topology(self.sysfs, [0, 1, 2, 3])
        pinning = choose_cpu_pinning(topology, "/usr/bin/taskset")

        self.assertEqual(topology["classification"], "heterogeneous")
        self.assertEqual(topology["fast_cores"], [0, 3])
        self.assertEqual(
            pinning["command_prefix"], ["/usr/bin/taskset", "--cpu-list", "0,3"]
        )

    def test_affinity_restriction_limits_topology_probe(self):
        self.write_frequency(1, 3_000_000)
        self.write_frequency(3, 5_000_000)

        topology = detect_cpu_topology(self.sysfs, [1, 3])

        self.assertEqual(topology["available_cpus"], [1, 3])
        self.assertEqual(topology["fast_cores"], [3])

    def test_uniform_topology_stays_unpinned(self):
        self.write_frequency(0, 5_000_000)
        self.write_frequency(1, 5_000_000)

        topology = detect_cpu_topology(self.sysfs, [0, 1])
        pinning = choose_cpu_pinning(topology, "/usr/bin/taskset")

        self.assertEqual(topology["classification"], "uniform")
        self.assertFalse(pinning["applied"])

    def test_incomplete_topology_stays_unpinned(self):
        self.write_frequency(0, 5_000_000)

        topology = detect_cpu_topology(self.sysfs, [0, 1])
        pinning = choose_cpu_pinning(topology, "/usr/bin/taskset")

        self.assertEqual(topology["classification"], "unreadable")
        self.assertIn("CPUs 1", topology["reason"])
        self.assertFalse(pinning["applied"])

    def test_missing_taskset_stays_unpinned(self):
        self.write_frequency(0, 5_000_000)
        self.write_frequency(1, 3_000_000)
        topology = detect_cpu_topology(self.sysfs, [0, 1])

        pinning = choose_cpu_pinning(topology, "")

        self.assertFalse(pinning["applied"])
        self.assertIn("taskset is unavailable", pinning["reason"])

    def test_cpu_list_uses_compact_ranges(self):
        self.assertEqual(format_cpu_list([15, 0, 2, 1, 12, 13]), "0-2,12-13,15")


class RepeatSummaryTests(unittest.TestCase):
    def test_median_and_range_are_reported(self):
        summary = summarize_repeats([100.0, 103.0, 105.0])

        self.assertEqual(summary["n"], 3)
        self.assertEqual(summary["median"], 103.0)
        self.assertEqual(summary["min"], 100.0)
        self.assertEqual(summary["max"], 105.0)
        self.assertFalse(summary["unstable"])

    def test_greater_than_five_percent_is_unstable(self):
        summary = summarize_repeats([100.0, 101.0, 105.0001])

        self.assertTrue(summary["unstable"])

    def test_invalid_values_are_rejected(self):
        for values in ([], [0.0, 1.0], [float("inf"), 1.0]):
            with self.subTest(values=values), self.assertRaises(ValueError):
                summarize_repeats(values)


class ScoreAggregationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.runner = load_runner_module()

    def measurement(self, results, worst_case_count=0):
        return {"scores": self.runner.compute_scores(results, worst_case_count)}

    def test_single_measurement_scores_are_self_consistent(self):
        scores = self.runner.compute_scores([100.0, 200.0, 400.0], 1)

        assert_scores_are_self_consistent(self, scores)
        self.assertIsNotNone(scores["worst_score"])

    def test_aggregate_of_divergent_measurements_stays_self_consistent(self):
        # Chosen so a per-field median of the *scores* would contradict the
        # geometric mean beside it: the first/average orderings disagree.
        measurements = [
            self.measurement([100.0, 100.0]),
            self.measurement([200.0, 400.0]),
            self.measurement([300.0, 200.0]),
        ]

        scores = self.runner.median_scores(measurements)

        assert_scores_are_self_consistent(self, scores)
        self.assertEqual(scores["first_time"], 200.0)
        self.assertEqual(scores["average_time"], 200.0)

    def test_aggregate_medians_the_times(self):
        measurements = [
            self.measurement([10.0, 30.0, 50.0], 1),
            self.measurement([20.0, 60.0, 100.0], 1),
            self.measurement([90.0, 270.0, 450.0], 1),
        ]

        scores = self.runner.median_scores(measurements)

        self.assertEqual(scores["first_time"], 20.0)
        self.assertEqual(scores["worst_time"], 100.0)
        assert_scores_are_self_consistent(self, scores)

    def test_absent_worst_case_stays_absent_after_aggregation(self):
        measurements = [self.measurement([10.0, 20.0]) for _ in range(3)]

        scores = self.runner.median_scores(measurements)

        self.assertIsNone(scores["worst_time"])
        self.assertIsNone(scores["worst_score"])
        assert_scores_are_self_consistent(self, scores)


class RunnerMeasurementTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.runner = load_runner_module()

    def test_busy_check_is_repeated_between_measurements(self):
        passing = {
            "name": "sample",
            "status": "pass",
            "elapsed": 0.1,
            "iterations": 2,
            "scores": {"overall_score": 100.0},
            "raw_times": [10.0, 20.0],
        }
        with (
            mock.patch.object(
                self.runner,
                "require_idle",
                side_effect=[(0.1, 0.2, 0.3), BusyHostError(2.0, 1.5)],
            ) as idle,
            mock.patch.object(
                self.runner, "run_benchmark_once", return_value=passing.copy()
            ) as run_once,
        ):
            result = self.runner.run_benchmark(
                "sample",
                "sync",
                [],
                None,
                2,
                False,
                0,
                ["engine"],
                "/tmp",
                30,
                False,
                None,
                3,
                1.5,
            )

        self.assertEqual(result["status"], "busy")
        self.assertEqual(result["completed_repeats"], 1)
        self.assertEqual(idle.call_count, 2)
        run_once.assert_called_once()

    def test_host_fingerprint_contains_required_comparability_fields(self):
        topology = {
            "available_cpus": [0, 1],
            "classification": "heterogeneous",
            "max_frequencies_khz": {0: 5_000_000, 1: 3_000_000},
            "fast_cores": [0],
        }
        pinning = {
            "applied": True,
            "cpu_list": "0",
            "reason": "pinned for test",
        }
        with (
            mock.patch("benchmark_protocol.read_cpu_model", return_value="Test CPU"),
            mock.patch("benchmark_protocol.read_nproc", return_value=2),
            mock.patch("benchmark_protocol.platform.node", return_value="test-host"),
        ):
            host = collect_host_fingerprint(topology, pinning, (0.1, 0.2, 0.3))

        self.assertEqual(host["cpu_model"], "Test CPU")
        self.assertEqual(host["nproc"], 2)
        self.assertEqual(host["loadavg_start"]["one_minute"], 0.1)
        self.assertEqual(host["fast_cores"], [0])
        self.assertTrue(host["pinning"]["applied"])


class RunnerCliTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.jetstream = self.root / "JetStream"
        (self.jetstream / "simple").mkdir(parents=True)
        (self.jetstream / "JetStreamDriver.js").touch()
        (self.jetstream / "simple" / "hash-map.js").write_text(
            "class Benchmark {}\n", encoding="utf-8"
        )
        self.counter = self.root / "counter"
        self.engine = self.root / "fake-engine.py"
        self.engine.write_text(
            textwrap.dedent(
                f"""\
                #!{sys.executable}
                import json
                import os
                from pathlib import Path

                counter = Path(os.environ["FAKE_JETSTREAM_COUNTER"])
                value = int(counter.read_text() if counter.exists() else "0") + 1
                counter.write_text(str(value))
                print(json.dumps({{
                    "results": [10 + value, 20 + value],
                    "iterations": 2,
                    "worstCaseCount": 0,
                }}))
                """
            ),
            encoding="utf-8",
        )
        self.engine.chmod(self.engine.stat().st_mode | stat.S_IXUSR)

    def tearDown(self):
        self.tmp.cleanup()

    def run_runner(self, *extra_args: str):
        env = os.environ | {"FAKE_JETSTREAM_COUNTER": str(self.counter)}
        return subprocess.run(
            [
                sys.executable,
                str(RUNNER_PATH),
                "--engine",
                str(self.engine),
                "--jetstream",
                str(self.jetstream),
                "--test",
                "hash-map",
                "--iterations",
                "2",
                *extra_args,
            ],
            cwd=self.root,
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )

    def test_cli_runs_three_measurements_and_records_host(self):
        output_path = self.root / "results.json"

        result = self.run_runner("--no-idle-gate", "--json", str(output_path))

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("N=3", result.stdout)
        self.assertIn("range:", result.stdout)
        self.assertIn("idle-window gate disabled", result.stdout)
        output = json.loads(output_path.read_text(encoding="utf-8"))
        benchmark = output["results"][0]
        self.assertEqual(benchmark["repeat_summary"]["n"], 3)
        assert_scores_are_self_consistent(self, benchmark["scores"])
        self.assertEqual(len(benchmark["measurements"]), 3)
        self.assertEqual(output["measurement_protocol"]["repeats"], 3)
        self.assertIn("cpu_model", output["host"])
        self.assertIn("nproc", output["host"])
        self.assertIn("loadavg_start", output["host"])
        self.assertEqual(self.counter.read_text(encoding="utf-8"), "3")

    def test_busy_host_exits_distinctly_and_keeps_existing_baseline(self):
        if read_load_averages()[0] == 0.0:
            self.skipTest("host reports zero load, so no threshold can trip")

        baseline = self.root / "jetstream-results.json"
        baseline.write_text('{"overall_score": 1234.5}', encoding="utf-8")

        # Any nonzero load exceeds a zero threshold, so the gate always trips.
        result = self.run_runner("--idle-threshold", "0")

        self.assertEqual(result.returncode, 3, result.stderr)
        self.assertNotEqual(result.returncode, 2)
        self.assertIn("BUSY", result.stdout)
        self.assertIn("existing baseline preserved", result.stdout)
        self.assertEqual(
            json.loads(baseline.read_text(encoding="utf-8")),
            {"overall_score": 1234.5},
        )

    def test_busy_host_keeps_baseline_when_json_aliases_default(self):
        if read_load_averages()[0] == 0.0:
            self.skipTest("host reports zero load, so no threshold can trip")

        baseline = self.root / "jetstream-results.json"
        alias = self.root / "results-alias.json"
        alias.symlink_to(baseline.name)
        json_targets = [
            "jetstream-results.json",
            str(baseline.resolve()),
            str(alias),
        ]

        for json_target in json_targets:
            with self.subTest(json_target=json_target):
                baseline.write_text('{"overall_score": 1234.5}', encoding="utf-8")

                result = self.run_runner("--idle-threshold", "0", "--json", json_target)

                self.assertEqual(result.returncode, 3, result.stderr)
                self.assertIn("existing baseline preserved", result.stdout)
                self.assertEqual(
                    json.loads(baseline.read_text(encoding="utf-8")),
                    {"overall_score": 1234.5},
                )

    def test_busy_host_writes_distinct_explicit_json(self):
        if read_load_averages()[0] == 0.0:
            self.skipTest("host reports zero load, so no threshold can trip")

        baseline = self.root / "jetstream-results.json"
        baseline.write_text('{"overall_score": 1234.5}', encoding="utf-8")
        partial = self.root / "busy-partial.json"

        result = self.run_runner("--idle-threshold", "0", "--json", str(partial))

        self.assertEqual(result.returncode, 3, result.stderr)
        self.assertEqual(
            json.loads(baseline.read_text(encoding="utf-8")),
            {"overall_score": 1234.5},
        )
        output = json.loads(partial.read_text(encoding="utf-8"))
        self.assertTrue(output["measurement_protocol"]["interrupted_by_busy_host"])
        self.assertEqual(output["passed"], 0)

    def test_repeats_below_three_are_rejected(self):
        result = self.run_runner("--repeats", "2", "--no-idle-gate")

        self.assertEqual(result.returncode, 2)
        self.assertIn("--repeats must be at least 3", result.stderr)

    def test_parallel_run_requires_idle_gate_opt_out(self):
        result = self.run_runner("-j", "2")

        self.assertEqual(result.returncode, 2)
        self.assertIn("requires --no-idle-gate", result.stderr)


if __name__ == "__main__":
    unittest.main()
