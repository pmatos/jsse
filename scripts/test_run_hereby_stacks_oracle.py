import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
ORACLE = SCRIPTS_DIR / "run-hereby-stacks-oracle.py"
FIXTURES = SCRIPTS_DIR / "hereby-fixtures"
KNOWN = SCRIPTS_DIR.parent / "ts-suite" / "hereby-stacks-known-failures.json"

spec = importlib.util.spec_from_file_location("run_hereby_stacks_oracle", ORACLE)
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)


class CaseTableTest(unittest.TestCase):
    def test_names_are_unique(self):
        names = [c[0] for c in oracle.CASES]
        self.assertEqual(len(names), len(set(names)))

    def test_every_fixture_exists(self):
        for name, fixture, *_ in oracle.CASES:
            self.assertTrue((FIXTURES / fixture / "Herebyfile.mjs").is_file(), name)

    def test_cases_run_no_task(self):
        for name, _, args, *_ in oracle.CASES:
            self.assertEqual(args, [], name)

    def test_known_failures_name_cases_and_issues(self):
        known = oracle.read_known(KNOWN)
        names = {c[0] for c in oracle.CASES}
        for case, issue in known.items():
            self.assertIn(case, names)
            self.assertRegex(issue, r"^#\d+$")
        self.assertTrue(set(json.loads(KNOWN.read_text())).issuperset({"issue"}))


class RatchetTest(unittest.TestCase):
    def test_unlisted_difference_fails(self):
        self.assertIn("differs", oracle.judge("a", True, {}))

    def test_listed_difference_is_accepted(self):
        self.assertIsNone(oracle.judge("a", True, {"a": "#1"}))

    def test_listed_case_that_agrees_fails(self):
        self.assertIn("#1", oracle.judge("a", False, {"a": "#1"}))

    def test_agreement_is_accepted(self):
        self.assertIsNone(oracle.judge("a", False, {}))


class LauncherTest(unittest.TestCase):
    def test_launcher_lifts_the_stack_limit_and_hands_hereby_its_argv(self):
        self.assertIn("Error.stackTraceLimit = 200", oracle.LAUNCHER)
        self.assertIn("process.argv.splice(1, 1)", oracle.LAUNCHER)


class SkipTest(unittest.TestCase):
    def run_oracle(self, env_extra, *args):
        env = {k: v for k, v in os.environ.items() if k != "TS_SRC_DIR"}
        env.update(env_extra)
        return subprocess.run(
            [sys.executable, str(ORACLE), *args],
            env=env,
            capture_output=True,
            text=True,
        )

    def test_skips_without_a_checkout(self):
        r = self.run_oracle({})
        self.assertEqual(r.returncode, 0)
        self.assertIn("SKIP: TS_SRC_DIR is not set", r.stdout)

    def test_require_fails_without_a_checkout(self):
        r = self.run_oracle({}, "--require")
        self.assertEqual(r.returncode, 3)

    def test_skips_a_checkout_without_hereby(self):
        with tempfile.TemporaryDirectory() as d:
            r = self.run_oracle({"TS_SRC_DIR": d})
        self.assertEqual(r.returncode, 0)
        self.assertIn("has no node_modules/hereby", r.stdout)

    def test_unknown_case_is_an_error(self):
        self.assertNotEqual(self.run_oracle({}, "nope").returncode, 0)


if __name__ == "__main__":
    unittest.main()
