import importlib.util
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
ORACLE = SCRIPTS_DIR / "run-hereby-oracle.py"
FIXTURES = SCRIPTS_DIR / "hereby-fixtures"

spec = importlib.util.spec_from_file_location("run_hereby_oracle", ORACLE)
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)


class NormaliseTest(unittest.TestCase):
    def norm(self, text):
        return oracle.normalise(text, "/w/node", "/ts")

    def test_paths_durations_and_pids(self):
        text = "Using /w/node/a/Herebyfile.mjs in /ts\nFinished x in 1.5s\nDone in 2m 3s\n(node:42) Warning"
        self.assertEqual(
            self.norm(text),
            "Using <WORK>/a/Herebyfile.mjs in <TS>\nFinished x in <T>\nDone in <T>\n(node:<PID>) Warning\n",
        )

    def test_only_durations_after_in_are_replaced(self):
        self.assertEqual(self.norm("took 5ms to run 3s\n"), "took 5ms to run 3s\n")

    def test_drops_frames_jsse_lacks(self):
        text = "\n".join(
            [
                "RangeError: boom",
                "    at explode (file:///w/node/a.mjs:4:9)",
                "    at file:///w/node/a.mjs:8:1",
                "    at ModuleJob.run (node:internal/modules/esm/module_job:569:25)",
                "    at async loadHerebyfile (file:///ts/node_modules/hereby/dist/cli.js:35:24)",
            ]
        )
        self.assertEqual(
            self.norm(text),
            "RangeError: boom\n    at explode (file://<WORK>/a.mjs:4:9)\n",
        )

    def test_keeps_anonymous_frames_that_are_not_the_module_top_level(self):
        text = "    at file:///w/node/a.mjs:3:5\n    at k (file:///w/node/a.mjs:2:15)\n"
        self.assertEqual(
            self.norm(text),
            "    at file://<WORK>/a.mjs:3:5\n    at k (file://<WORK>/a.mjs:2:15)\n",
        )

    def test_keeps_named_frames_and_plain_lines(self):
        text = "    at k (/w/node/a.js:2:15)\nat async not a frame\n"
        self.assertEqual(
            self.norm(text), "    at k (<WORK>/a.js:2:15)\nat async not a frame\n"
        )


class CaseTableTest(unittest.TestCase):
    def test_names_are_unique(self):
        names = [c[0] for c in oracle.HEREBY_CASES + oracle.DEPS_CASES]
        self.assertEqual(len(names), len(set(names)))

    def test_every_fixture_directory_exists(self):
        for name, fixture, *_ in oracle.HEREBY_CASES:
            if fixture and fixture != "empty":
                self.assertTrue((FIXTURES / fixture / "Herebyfile.mjs").is_file(), name)
        self.assertTrue((FIXTURES / "deps" / "deps.mjs").is_file())

    def test_no_case_runs_a_build(self):
        for name, fixture, args, *_ in oracle.HEREBY_CASES:
            if not fixture:
                self.assertTrue(
                    any(
                        a.startswith("-") or a in ("no-such-task", "build-sr")
                        for a in args
                    ),
                    name,
                )


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
        self.assertIn("ERROR: TS_SRC_DIR is not set", r.stdout)

    def test_skips_a_checkout_without_hereby(self):
        with tempfile.TemporaryDirectory() as d:
            r = self.run_oracle({"TS_SRC_DIR": d})
        self.assertEqual(r.returncode, 0)
        self.assertIn("has no node_modules/hereby", r.stdout)

    def test_skips_another_typescript_version(self):
        with tempfile.TemporaryDirectory() as d:
            hereby = Path(d, "node_modules", "hereby", "bin")
            hereby.mkdir(parents=True)
            (hereby / "hereby.js").write_text("")
            Path(d, "package.json").write_text(
                '{"name": "typescript", "version": "5.0.0"}'
            )
            r = self.run_oracle({"TS_SRC_DIR": d})
        self.assertEqual(r.returncode, 0)
        self.assertIn("found 5.0.0", r.stdout)

    def test_unknown_case_is_an_error(self):
        r = self.run_oracle({}, "nope")
        self.assertNotEqual(r.returncode, 0)


if __name__ == "__main__":
    unittest.main()
