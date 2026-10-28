import importlib.util
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
ORACLE = SCRIPTS_DIR / "run-ts-mocha-oracle.py"
FIXTURES = SCRIPTS_DIR / "ts-mocha-fixtures"

spec = importlib.util.spec_from_file_location("run_ts_mocha_oracle", ORACLE)
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)


class NormaliseTest(unittest.TestCase):
    def norm(self, text):
        return oracle.normalise(text, "/w/node", "/ts")

    def test_paths_durations_and_pids(self):
        text = "  3 passing (12ms)\n(node:42) Warning\n  list: 5ms\nin /w/node/a and /ts/b\n"
        self.assertEqual(
            self.norm(text),
            "  3 passing (<T>)\n(node:<PID>) Warning\n  list: <T>\nin <WORK>/a and <TS>/b\n",
        )

    def test_slow_marks_on_passing_lines_are_dropped(self):
        self.assertEqual(
            self.norm("    ✔ one (38ms)\n  1 passing (40ms)\n"),
            "    ✔ one\n  1 passing (<T>)\n",
        )

    def test_drops_frames_in_node_builtins_and_async_frames(self):
        text = "\n".join(
            [
                "Error: boom",
                "    at k (/w/node/a.js:2:15)",
                "    at process.emit (node:events:514:20)",
                "    at listOnTimeout (node:internal/timers:685:17)",
                "    at async run (/ts/x.js:1:1)",
                "    at node:internal/timers:40:7 {",
            ]
        )
        self.assertEqual(self.norm(text), "Error: boom\n    at k (a.js:2:15)\n{\n")

    def test_arrow_receivers_and_relative_frames_read_alike(self):
        node = "    at Context.<anonymous> (a.spec.js:5:36)\n    at Timeout._onTimeout (a.spec.js:2:9)\n"
        jsse = "    at /w/node/a.spec.js:5:36\n    at /w/node/a.spec.js:2:9\n"
        self.assertEqual(self.norm(node), self.norm(jsse))

    def test_named_functions_keep_their_names(self):
        text = "    at k (/w/node/a.js:2:15)\n    at Foo.bar (/w/node/a.js:3:1)\n"
        self.assertEqual(
            self.norm(text),
            "    at k (a.js:2:15)\n    at Foo.bar (a.js:3:1)\n",
        )

    def test_xunit_closing_tags_survive_a_dropped_frame(self):
        text = "    at x (/w/node/a.js:1:1)\n    at process.processImmediate (node:internal/timers:574:21)</failure></testcase>\n"
        self.assertEqual(
            self.norm(text), "    at x (a.js:1:1)\n</failure></testcase>\n"
        )

    def test_json_is_compared_by_key_not_by_property_order(self):
        a = '{"err": {"stack": "E\\n    at f (/w/node/a.js:1:1)", "message": "m"}, "duration": 3}'
        b = '{"duration": 9, "err": {"message": "m", "stack": "E\\n    at f (/w/node/a.js:1:1)"}}'
        one = oracle.normalise(
            oracle.normalise_json(a, "/w/node", "/ts"), "/w/node", "/ts"
        )
        two = oracle.normalise(
            oracle.normalise_json(b, "/w/node", "/ts"), "/w/node", "/ts"
        )
        self.assertEqual(one, two)


class TrailingBraceAfterFrameTest(unittest.TestCase):
    def test_a_brace_after_a_dropped_frame_reads_like_one_after_another(self):
        node = "Error: x\n    at k (/w/a.js:1:1)\n    at async y (/ts/z.js:2:2) {\n  code: 'E'\n}\n"
        jsse = "Error: x\n    at k (/w/a.js:1:1)\n    at node:internal/boot:4:5 {\n  code: 'E'\n}\n"
        self.assertEqual(
            oracle.normalise(node, "/w", "/ts"), oracle.normalise(jsse, "/w", "/ts")
        )


class JsonReportersTest(unittest.TestCase):
    def norm(self, text):
        return oracle.normalise_json(text, "/w", "/ts")

    def test_text_printed_before_the_report_is_kept(self):
        text = 'beforeEach 1\n{"stats": {"duration": 4}, "n": 1}\n'
        out = self.norm(text)
        self.assertTrue(out.startswith("beforeEach 1\n{"))
        self.assertIn('"duration": "<T>"', out)

    def test_json_stream_lines_are_normalised_one_by_one(self):
        a = '["start",{"total":1}]\n["fail",{"duration":7,"err":{"stack":"E\\n    at f (/w/a.js:1:1)","message":"m"}}]\n'
        b = '["start",{"total":1}]\n["fail",{"duration":2,"err":{"message":"m","stack":"E\\n    at f (/w/a.js:1:1)"}}]\n'
        self.assertEqual(
            oracle.normalise_json(a, "/w", "/ts", stream=True),
            oracle.normalise_json(b, "/w", "/ts", stream=True),
        )

    def test_text_printed_after_the_report_does_not_stop_normalising(self):
        out = self.norm('{"stats": {"duration": 4}}\nlate output\n')
        self.assertIn('"duration": "<T>"', out)
        self.assertTrue(out.endswith("late output\n"))

    def test_speed_survives_but_timings_do_not(self):
        out = self.norm('{"tests": [{"duration": 3, "speed": "slow"}]}')
        self.assertIn('"speed": "slow"', out)
        self.assertNotIn("3", out)


class CaseTableTest(unittest.TestCase):
    def test_names_are_unique(self):
        names = [c[0] for c in oracle.CASES]
        self.assertEqual(len(names), len(set(names)))

    def test_every_fixture_exists(self):
        for name, _, specs, *_ in oracle.CASES:
            for spec_file in specs:
                if spec_file not in ("RUN", "absent.spec.js"):
                    self.assertTrue((FIXTURES / spec_file).is_file(), name)

    def test_the_timing_and_failure_output_cases_exist(self):
        names = {c[0] for c in oracle.CASES}
        for want in (
            "timers-spec",
            "timeouts-json",
            "cli-timeout-60ms",
            "diff-colors",
            "retries-cli-spec",
            "fail-fivemat",
            "fail-full-trace",
            "list-reporters",
        ):
            self.assertIn(want, names)

    def test_runs_stay_small(self):
        for path in FIXTURES.iterdir():
            self.assertLess(path.stat().st_size, 20000, path.name)


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

    def test_skips_a_checkout_without_mocha(self):
        with tempfile.TemporaryDirectory() as d:
            r = self.run_oracle({"TS_SRC_DIR": d})
        self.assertEqual(r.returncode, 0)
        self.assertIn("has no node_modules/mocha", r.stdout)

    def test_skips_an_unbuilt_checkout(self):
        with tempfile.TemporaryDirectory() as d:
            mocha = Path(d, "node_modules", "mocha", "bin")
            mocha.mkdir(parents=True)
            (mocha / "_mocha").write_text("")
            Path(d, "package.json").write_text('{"version": "6.0.3"}')
            r = self.run_oracle({"TS_SRC_DIR": d})
        self.assertEqual(r.returncode, 0)
        self.assertIn("has no built/local/run.js", r.stdout)

    def test_skips_another_typescript_version(self):
        with tempfile.TemporaryDirectory() as d:
            mocha = Path(d, "node_modules", "mocha", "bin")
            mocha.mkdir(parents=True)
            (mocha / "_mocha").write_text("")
            built = Path(d, "built", "local")
            built.mkdir(parents=True)
            (built / "run.js").write_text("")
            Path(d, "package.json").write_text('{"version": "5.0.0"}')
            r = self.run_oracle({"TS_SRC_DIR": d})
        self.assertEqual(r.returncode, 0)
        self.assertIn("targets TypeScript 6.0.3, found 5.0.0", r.stdout)

    def test_unknown_case_is_an_error(self):
        r = self.run_oracle({"TS_SRC_DIR": "/nonexistent"}, "no-such-case")
        self.assertNotEqual(r.returncode, 0)

    def test_a_negated_glob_removes_cases(self):
        r = self.run_oracle({}, "--list", "!ts-*", "!fail-*")
        self.assertEqual(r.returncode, 0)
        names = r.stdout.split()
        self.assertIn("pass-spec", names)
        self.assertFalse([n for n in names if n.startswith(("ts-", "fail-"))])

    def test_list_prints_case_names(self):
        r = self.run_oracle({}, "--list", "pass-*")
        self.assertEqual(r.returncode, 0)
        self.assertIn("pass-spec", r.stdout)
        self.assertNotIn("fail-spec", r.stdout)


if __name__ == "__main__":
    unittest.main()
