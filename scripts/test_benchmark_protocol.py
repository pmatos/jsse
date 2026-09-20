import importlib.util
import io
import json
import math
import os
import re
import shutil
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


def engine_command_or_skip(case):
    """Real engine when built, else node: the generated JS is engine-neutral."""
    engine = REPO_ROOT / "target" / "release" / "jsse"
    if engine.exists():
        return [str(engine)]
    if shutil.which("node"):
        return ["node"]
    case.skipTest("neither target/release/jsse nor node is available")


def run_js(case, program, timeout=60):
    with tempfile.TemporaryDirectory() as tmp:
        script = Path(tmp) / "program.js"
        script.write_text(program, encoding="utf-8")
        return subprocess.run(
            engine_command_or_skip(case) + [str(script)],
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )


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


class HarnessNameHygieneTests(unittest.TestCase):
    """The generated harness shares one Script with the benchmark sources.

    A top-level `const benchmark` next to a benchmark's own top-level
    `function benchmark()` is an early SyntaxError per ECMAScript 16.1.1, so
    the harness must not introduce any top-level binding (issue #652).
    """

    TOP_LEVEL_DECLARATION = r"^(?:const|let|var|class|function|async\s+function)\b"

    @classmethod
    def setUpClass(cls):
        cls.runner = load_runner_module()

    def test_harnesses_declare_nothing_at_top_level(self):
        for name, harness in (
            ("preamble", self.runner.build_polyfill_preamble()),
            ("random", self.runner.build_deterministic_random_code()),
            ("sync", self.runner.build_sync_harness(1, False, 3)),
            ("async", self.runner.build_async_harness(1, False, 3)),
        ):
            with self.subTest(harness=name):
                self.assertIsNone(
                    re.search(self.TOP_LEVEL_DECLARATION, harness, re.M),
                    harness,
                )

    def test_sync_harness_runs_beside_colliding_benchmark_names(self):
        program = (
            self.runner.build_polyfill_preamble()
            + textwrap.dedent(
                """
                var __iterations = "workload";
                function __results() {}
                function benchmark() { return 1; }
                var Benchmark = class {
                    runIteration() { benchmark(); }
                }
                """
            )
            + self.runner.build_sync_harness(1, False, 3)
        )

        result = run_js(self, program)

        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout.strip().splitlines()[-1])
        self.assertEqual(len(payload["results"]), 1)


class PolyfillPreambleTests(unittest.TestCase):
    """JetStream's shell driver runs benchmarks with `self === globalThis`.

    Sources such as bigint-paillier and the noble-* bundles probe `self` and
    fail with a ReferenceError when the runner's prelude does not define it.
    """

    @classmethod
    def setUpClass(cls):
        cls.runner = load_runner_module()

    def test_self_aliases_the_global_object(self):
        program = self.runner.build_polyfill_preamble() + textwrap.dedent(
            """
            print(typeof self, self === globalThis);
            """
        )

        result = run_js(self, program)

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.split(), ["object", "true"])

    def test_self_is_readable_as_a_bare_identifier_in_a_function(self):
        program = self.runner.build_polyfill_preamble() + textwrap.dedent(
            """
            function probe() {
                return typeof self === "object" && "Math" in self;
            }
            print(probe());
            """
        )

        result = run_js(self, program)

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "true")

    def test_print_err_does_not_throw_when_console_has_no_error(self):
        program = (
            'Object.defineProperty(console, "error", '
            "{ value: undefined, configurable: true });\n"
            + self.runner.build_polyfill_preamble()
            + 'printErr("diagnostic");\nprint("survived");\n'
        )

        result = run_js(self, program)

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("survived", result.stdout)

    def test_existing_self_is_not_clobbered(self):
        program = (
            "globalThis.self = 42;\n"
            + self.runner.build_polyfill_preamble()
            + "print(self);\n"
        )

        result = run_js(self, program)

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "42")


class PreloadShimTests(unittest.TestCase):
    """JetStream 3 workloads read preloaded resources through a `JetStream`
    object (`JetStream.preload.<name>` paths resolved by `getString`), not
    through bare globals; the runner must provide that object (issue #655).
    """

    TRICKY_CONTENT = (
        "`tick` ${notInterpolated} back\\slash \\` </script> line\u2028sep\r\n"
        "crlf caf\u00e9 \U0001f600 \"quoted\" 'single'\n"
    )

    @classmethod
    def setUpClass(cls):
        cls.runner = load_runner_module()

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / "dir").mkdir()
        (self.root / "dir" / "blob.js").write_bytes(self.TRICKY_CONTENT.encode("utf-8"))

    def run_with_shim(self, preloads, check):
        code = self.runner.build_preload_code(preloads, str(self.root))
        program = code + "\n" + textwrap.dedent(check)
        return run_js(self, program)

    def test_get_string_returns_the_file_bytes_verbatim(self):
        result = self.run_with_shim(
            {"blob": "dir/blob.js"},
            f"""
            (async () => {{
                const text = await JetStream.getString(JetStream.preload.blob);
                console.log(JSON.stringify({{ same: text === {json.dumps(self.TRICKY_CONTENT)} }}));
            }})();
            """,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), {"same": True})

    def test_preload_maps_each_name_to_a_path_that_get_string_accepts(self):
        (self.root / "dir" / "other.js").write_text("other", encoding="utf-8")

        result = self.run_with_shim(
            {"blob": "dir/blob.js", "other": "dir/other.js"},
            """
            (async () => {
                const names = Object.keys(JetStream.preload).sort();
                const other = await JetStream.getString(JetStream.preload.other);
                console.log(JSON.stringify({ names, other }));
            })();
            """,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            json.loads(result.stdout), {"names": ["blob", "other"], "other": "other"}
        )

    def test_get_string_rejects_a_path_that_was_not_preloaded(self):
        result = self.run_with_shim(
            {"blob": "dir/blob.js"},
            """
            JetStream.getString("nope.js").then(
                () => console.log("resolved"),
                (e) => console.log("rejected: " + e.message),
            );
            """,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("rejected:", result.stdout)
        self.assertIn("nope.js", result.stdout)

    def test_get_binary_rejects_as_unsupported_by_the_runner(self):
        result = self.run_with_shim(
            {"blob": "dir/blob.js"},
            """
            JetStream.getBinary(JetStream.preload.blob).then(
                () => console.log("resolved"),
                (e) => console.log("rejected: " + e.message),
            );
            """,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("rejected:", result.stdout)
        self.assertIn("run-jetstream.py", result.stdout)

    def test_missing_preload_file_yields_none(self):
        self.assertIsNone(
            self.runner.build_preload_code({"gone": "dir/missing.js"}, str(self.root))
        )

    def test_preload_code_declares_nothing_at_top_level(self):
        code = self.runner.build_preload_code({"blob": "dir/blob.js"}, str(self.root))

        self.assertIsNone(
            re.search(HarnessNameHygieneTests.TOP_LEVEL_DECLARATION, code, re.M),
            code,
        )


class AsyncHarnessRejectionTests(unittest.TestCase):
    """An async benchmark that rejects must say so on stdout and stderr.

    A shell drops an unobserved rejection and exits 0 with no output, which
    the runner used to report as an opaque "no JSON output" (issue #655).
    """

    @classmethod
    def setUpClass(cls):
        cls.runner = load_runner_module()

    def run_benchmark_source(self, source):
        program = (
            self.runner.build_polyfill_preamble()
            + textwrap.dedent(source)
            + self.runner.build_async_harness(1, False, 3)
        )
        return run_js(self, program)

    def test_harness_attaches_a_rejection_handler(self):
        harness = self.runner.build_async_harness(1, False, 3)

        self.assertIn(".catch(", harness)
        self.assertIn("printErr(", harness)

    def test_init_rejection_prints_a_json_error_line(self):
        result = self.run_benchmark_source(
            """
            class Benchmark {
                async init() { throw new Error("boom"); }
                runIteration() {}
            }
            """
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout.strip().splitlines()[-1])
        self.assertIn("boom", payload["error"])

    def test_non_error_rejection_value_is_reported(self):
        result = self.run_benchmark_source(
            """
            class Benchmark {
                runIteration() { return Promise.reject("plain string"); }
            }
            """
        )

        payload = json.loads(result.stdout.strip().splitlines()[-1])
        self.assertIn("plain string", payload["error"])

    def test_successful_run_still_prints_only_results(self):
        result = self.run_benchmark_source(
            """
            class Benchmark {
                async runIteration() {}
            }
            """
        )

        payload = json.loads(result.stdout.strip().splitlines()[-1])
        self.assertNotIn("error", payload)
        self.assertEqual(len(payload["results"]), 1)
        self.assertEqual(result.stderr, "")

    def test_async_harness_declares_nothing_at_top_level(self):
        harness = self.runner.build_async_harness(1, False, 3)

        self.assertIsNone(
            re.search(HarnessNameHygieneTests.TOP_LEVEL_DECLARATION, harness, re.M),
            harness,
        )


def write_fake_engine(directory, body):
    """Executable stand-in for an engine; `body` is Python run per invocation."""
    engine = Path(directory) / "fake-engine.py"
    engine.write_text(
        f"#!{sys.executable}\nimport sys\n" + textwrap.dedent(body),
        encoding="utf-8",
    )
    engine.chmod(engine.stat().st_mode | stat.S_IXUSR)
    return [str(engine)]


class RunBenchmarkOnceFailureTests(unittest.TestCase):
    """Every failure result keeps enough output to diagnose it afterwards."""

    @classmethod
    def setUpClass(cls):
        cls.runner = load_runner_module()

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.jetstream = self.root / "JetStream"
        self.jetstream.mkdir()
        (self.jetstream / "bench.js").write_text("class Benchmark {}\n")

    def run_once(self, engine_body, btype="sync", verbose=False):
        return self.runner.run_benchmark_once(
            "bench",
            btype,
            ["bench.js"],
            None,
            1,
            False,
            0,
            write_fake_engine(self.root, engine_body),
            str(self.jetstream),
            30,
            verbose,
            None,
        )

    def test_error_line_becomes_an_error_result_with_the_message(self):
        result = self.run_once(
            """
            print('{"error": "Error: boom"}')
            sys.stderr.write("Error: boom\\n")
            """,
            btype="async",
        )

        self.assertEqual(result["status"], "error")
        self.assertIn("boom", result["reason"])
        self.assertIn("boom", result["stdout"])
        self.assertIn("boom", result["stderr"])

    def test_no_json_keeps_stdout_and_stderr(self):
        result = self.run_once(
            """
            print("chatter")
            sys.stderr.write("warn")
            """
        )

        self.assertEqual(result["status"], "error")
        self.assertEqual(result["reason"], "no JSON output")
        self.assertEqual(result["stdout"].strip(), "chatter")
        self.assertEqual(result["stderr"], "warn")

    def test_silent_async_exit_is_reported_as_never_settled(self):
        result = self.run_once("", btype="async")

        self.assertEqual(result["status"], "error")
        self.assertIn("never settled", result["reason"])
        self.assertNotEqual(result["reason"], "no JSON output")
        self.assertEqual(result["stderr"], "")

    def test_nonzero_exit_keeps_both_streams(self):
        result = self.run_once(
            """
            print("partial output")
            sys.stderr.write("SyntaxError: nope")
            sys.exit(1)
            """
        )

        self.assertEqual(result["status"], "error")
        self.assertEqual(result["reason"], "exit code 1")
        self.assertIn("partial output", result["stdout"])
        self.assertIn("SyntaxError: nope", result["stderr"])

    def test_malformed_json_line_keeps_the_output(self):
        result = self.run_once('print("{not json")')

        self.assertEqual(result["status"], "error")
        self.assertIn("JSON parse error", result["reason"])
        self.assertIn("{not json", result["stdout"])

    def test_empty_results_keep_the_output(self):
        result = self.run_once('print(\'{"results": [], "iterations": 0}\')')

        self.assertEqual(result["status"], "error")
        self.assertEqual(result["reason"], "no benchmark iterations")
        self.assertIn("results", result["stdout"])

    def test_oversized_output_is_clipped_keeping_head_and_tail(self):
        result = self.run_once(
            """
            sys.stderr.write("HEAD" + "x" * 100000 + "TAIL")
            sys.exit(1)
            """
        )

        stderr = result["stderr"]
        self.assertLess(len(stderr), 5000)
        self.assertTrue(stderr.startswith("HEAD"))
        self.assertTrue(stderr.endswith("TAIL"))
        self.assertIn("truncated", stderr)

    def test_short_output_is_not_marked_truncated(self):
        result = self.run_once('sys.stderr.write("short"); sys.exit(2)')

        self.assertEqual(result["stderr"], "short")

    def test_verbose_prints_output_for_a_silent_failure(self):
        with mock.patch("sys.stderr", new_callable=io.StringIO) as err:
            self.run_once(
                """
                print("chatter")
                sys.stderr.write("warn")
                """,
                verbose=True,
            )

        self.assertIn("warn", err.getvalue())
        self.assertIn("chatter", err.getvalue())

    def test_successful_run_is_unchanged(self):
        result = self.run_once(
            """
            print('{"results": [4, 6], "iterations": 2, "worstCaseCount": 0}')
            """
        )

        self.assertEqual(result["status"], "pass")
        self.assertEqual(result["raw_times"], [4, 6])


if __name__ == "__main__":
    unittest.main()
