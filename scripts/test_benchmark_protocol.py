import importlib.util
import json
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
        self.assertEqual(
            benchmark["scores"]["overall_score"],
            benchmark["repeat_summary"]["median"],
        )
        self.assertEqual(len(benchmark["measurements"]), 3)
        self.assertEqual(output["measurement_protocol"]["repeats"], 3)
        self.assertIn("cpu_model", output["host"])
        self.assertIn("nproc", output["host"])
        self.assertIn("loadavg_start", output["host"])
        self.assertEqual(self.counter.read_text(encoding="utf-8"), "3")

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
