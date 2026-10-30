import importlib.util
import json
import os
import subprocess
import sys
import unittest
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent
ROOT = SCRIPTS.parent
sys.path.insert(0, str(SCRIPTS))
import ts_budget_cases as cases  # noqa: E402
import ts_budget_check as check  # noqa: E402

ORACLE = SCRIPTS / "run-ts-budget-oracle.py"
spec = importlib.util.spec_from_file_location("run_ts_budget_oracle", ORACLE)
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)


def run(passed=("a", "b"), failed=(), secs=1.0, rss=100.0, local=()):
    return {
        "exit": "0",
        "seconds": secs,
        "rss_mb": rss,
        "summary": (list(passed), list(failed), 0),
        "local": list(local),
        "tail": "",
    }


def budget(rss=6, **extra):
    return {"c": {"max_rss_ratio": rss, **extra}}


class CompareTest(unittest.TestCase):
    def test_agreeing_runs(self):
        self.assertEqual(check.compare("c", run(), run(secs=90), set()), [])

    def test_a_test_lost_on_jsse_is_reported_unless_known(self):
        node, jsse = run(), run(passed=("a",), failed=("b",))
        self.assertEqual(len(check.compare("c", node, jsse, set())), 1)
        self.assertEqual(check.compare("c", node, jsse, {"b"}), [])

    def test_a_known_failure_that_passes_is_a_ratchet_error(self):
        lines = check.compare("c", run(), run(), {"b"})
        self.assertIn("known failure", lines[0])

    def test_node_must_pass_something_and_everything(self):
        self.assertTrue(check.compare("c", run(passed=()), run(passed=()), set()))
        self.assertTrue(check.compare("c", run(failed=("x",)), run(), set()))
        self.assertTrue(check.compare("c", run(local=("f",)), run(), set()))

    def test_a_missing_report_is_reported_with_the_output(self):
        broken = run()
        broken["summary"], broken["tail"] = None, "boom"
        lines = check.compare("c", run(), broken, set())
        self.assertIn("jsse produced no report", lines[0])
        self.assertIn("boom", lines[0])


class BudgetTest(unittest.TestCase):
    def test_rss_ratio_is_enforced(self):
        self.assertEqual(
            check.check_budget("c", run(rss=100), run(rss=600), budget()), []
        )
        lines = check.check_budget("c", run(rss=100), run(rss=601), budget())
        self.assertIn("6.01x Node", lines[0])

    def test_wall_time_is_recorded_not_enforced(self):
        self.assertEqual(
            check.check_budget("c", run(secs=1), run(secs=1e6), budget()), []
        )

    def test_optional_limits_apply_when_set(self):
        both = budget(jsse_max_seconds=10, jsse_max_rss_mb=50, max_time_ratio=2)
        lines = check.check_budget("c", run(secs=1, rss=10), run(secs=11, rss=60), both)
        self.assertEqual(len(lines), 3)

    def test_a_case_without_an_entry_is_an_error(self):
        self.assertTrue(check.check_budget("d", run(), run(), budget()))


class ShippedBudgetTest(unittest.TestCase):
    def setUp(self):
        with open(ROOT / "ts-suite" / "budget-1297.json", encoding="utf-8") as f:
            self.budget = json.load(f)

    def test_the_budget_covers_exactly_the_cases_with_the_ruled_limits(self):
        self.assertEqual(check.validate_budget(self.budget, cases.CASES), [])

    def test_validation_rejects_a_wall_time_cap_or_a_larger_rss_cap(self):
        bad = {n: dict(c) for n, c in self.budget.items()}
        bad["transpile"]["max_time_ratio"] = 10
        bad["tscwatch"]["max_rss_ratio"] = 7
        del bad["services"]
        lines = check.validate_budget(bad, cases.CASES)
        self.assertEqual(len(lines), 3)

    def test_every_case_selects_runners_and_a_valid_regex(self):
        import re

        for name, (runners, grep, _, why) in cases.CASES.items():
            self.assertTrue(runners and why, name)
            re.compile(grep)
        self.assertTrue(cases.SMOKE)
        self.assertLess(len(cases.SMOKE), len(cases.CASES))

    def test_check_budget_flag_needs_no_checkout(self):
        out = subprocess.run(
            [sys.executable, str(ORACLE), "--check-budget"],
            capture_output=True,
            text=True,
            env={k: v for k, v in os.environ.items() if k != "TS_SRC_DIR"},
        )
        self.assertEqual(out.returncode, 0, out.stdout + out.stderr)
        self.assertIn("budget OK", out.stdout)

    def test_without_a_checkout_the_run_skips_unless_required(self):
        env = {k: v for k, v in os.environ.items() if k != "TS_SRC_DIR"}
        skipped = subprocess.run(
            [sys.executable, str(ORACLE), "--smoke"],
            capture_output=True,
            text=True,
            env=env,
        )
        self.assertEqual(skipped.returncode, 0)
        self.assertIn("SKIP:", skipped.stdout)
        required = subprocess.run(
            [sys.executable, str(ORACLE), "--smoke", "--require"],
            capture_output=True,
            text=True,
            env=env,
        )
        self.assertEqual(required.returncode, 3)


class MeasureTest(unittest.TestCase):
    def test_peak_rss_and_exit_code_of_a_child_are_measured(self):
        code, out, secs, rss = check.run_measured(
            [sys.executable, "-c", "b = bytearray(64 * 1024 * 1024); print('hi')"],
            os.getcwd(),
            dict(os.environ),
            60,
        )
        self.assertEqual((code, out.strip()), ("0", b"hi"))
        self.assertGreater(rss, 60)
        self.assertGreater(secs, 0)

    def test_a_timeout_kills_the_group(self):
        code, _, secs, _ = check.run_measured(
            [sys.executable, "-c", "import time; time.sleep(60)"],
            os.getcwd(),
            dict(os.environ),
            0.3,
        )
        self.assertEqual(code, "timeout")
        self.assertLess(secs, 10)

    def test_oracle_exposes_the_driver_entry_points(self):
        self.assertTrue(callable(oracle.main))
        self.assertEqual(oracle.MAX_JOBS, 4)


if __name__ == "__main__":
    unittest.main()
