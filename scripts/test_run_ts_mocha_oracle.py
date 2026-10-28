import importlib.util
import os
import re
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
cli = oracle.cli


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


class CaseTableTest(unittest.TestCase):
    def test_names_are_unique(self):
        names = [c[0] for c in oracle.CASES]
        self.assertEqual(len(names), len(set(names)))

    def test_every_fixture_exists(self):
        for name, _, specs, *_ in oracle.CASES:
            for spec_file in specs:
                if spec_file not in ("RUN", "absent.spec.js"):
                    self.assertTrue((FIXTURES / spec_file).is_file(), name)

    def test_runs_stay_small(self):
        for path in FIXTURES.iterdir():
            self.assertLess(path.stat().st_size, 20000, path.name)


class CliCasesTest(unittest.TestCase):
    def cases(self):
        return [c for c in oracle.CASES if c.kind != "mocha"]

    def test_runtests_cases_run_the_driver_with_a_fixture_or_the_bundle(self):
        for c in self.cases():
            if c.kind == "runtests":
                self.assertEqual(c.specs[0], "runtests.mjs", c.name)
                self.assertEqual(len(c.specs) == 2, "TS_RUN_JS" in c.env, c.name)

    def test_listing_cases_are_dry_runs_and_the_others_are_not(self):
        for c in self.cases():
            if c.name.startswith(("cli-list-", "cli-tests-", "cli-shards-")):
                self.assertEqual(c.mocharc, {"dry-run": True}, c.name)
            else:
                self.assertIsNone(c.mocharc, c.name)

    def test_expectations_name_keys_a_result_can_have(self):
        keys = re.compile(r"^(then )?(stdout|stderr|file [\w. -]+)$")
        for c in oracle.CASES:
            for key, pattern in c.must:
                self.assertRegex(key, keys, c.name)
                re.compile(pattern)

    def test_partitions_name_existing_cases(self):
        names = {c.name for c in oracle.CASES}
        for whole, shards in cli.PARTITIONS.items():
            self.assertLessEqual({whole, *shards}, names)

    def test_failed_flow_cases_that_rerun_have_a_second_step(self):
        for c in self.cases():
            if "rerun" in c.name or "keep-without" in c.name:
                self.assertIsNotNone(c.then, c.name)

    def test_titles_skip_the_leaked_unit_test_and_summary_lines(self):
        out = "ok 1 unittests:: a\nok 2 compiler x\n# tests 2\nnot ok 3 y\nok 3 z\n"
        self.assertEqual(cli.titles(out), ["compiler x", "z"])

    def test_shards_that_do_not_add_up_are_reported(self):
        def res(name, *titles):
            out = "".join(f"ok {i} {t}\n" for i, t in enumerate(titles, 1))
            r = {"stdout": out}
            return (cli.Case(name, [], []), r, r, {})

        whole = res("cli-list-project", "a", "b", "c")
        a = res("cli-shards-project-1of3", "a")
        b = res("cli-shards-project-2of3", "b", "c")
        c = res("cli-shards-project-3of3")
        self.assertEqual(cli.partition_problems([whole, a, b, c]), [])
        short = res("cli-shards-project-2of3", "b")
        problems = cli.partition_problems([whole, a, short, c])
        self.assertEqual(len(problems), 2)
        self.assertEqual(cli.partition_problems([whole, a]), [])

    def test_tree_lists_files_with_content_hashes(self):
        with tempfile.TemporaryDirectory() as d:
            Path(d, "sub").mkdir()
            Path(d, "sub", "b.txt").write_text("hello\n")
            Path(d, "a.js").write_text("x")
            plain = cli.tree(d)
            self.assertEqual(plain.count("\n"), 2)
            self.assertTrue(plain.startswith("a.js 1 "))
            self.assertIn("sub/b.txt 6 ", plain)
            self.assertIn("sub/b.txt\nhello", cli.tree(d, content=True))

    def test_exec_lines_and_hereby_durations_are_normalised(self):
        self.assertEqual(
            oracle.EXEC_LINE.sub("> <EXEC> ", "> /bin/node mocha -t 1\nplain > x\n"),
            "> <EXEC> mocha -t 1\nplain > x\n",
        )
        self.assertEqual(
            oracle.HEREBY_TIME.sub("<T>", "Finished baseline-accept in 14ms"),
            "Finished baseline-accept in <T>",
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

    def test_list_prints_case_names(self):
        r = self.run_oracle({}, "--list", "pass-*")
        self.assertEqual(r.returncode, 0)
        self.assertIn("pass-spec", r.stdout)
        self.assertNotIn("fail-spec", r.stdout)


if __name__ == "__main__":
    unittest.main()
