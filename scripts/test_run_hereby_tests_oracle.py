import importlib.util
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ORACLE = Path(__file__).resolve().parent / "run-hereby-tests-oracle.py"

spec = importlib.util.spec_from_file_location("run_hereby_tests_oracle", ORACLE)
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)


class ArgvTest(unittest.TestCase):
    def test_jsse_gets_the_node_flag_and_node_does_not(self):
        node = oracle.hereby_argv("node", "/usr/bin/node", [])
        jsse = oracle.hereby_argv("jsse", "/opt/jsse", ["--no-typecheck"])
        self.assertEqual(node[0], "/usr/bin/node")
        self.assertNotIn("--node", node)
        self.assertEqual(jsse[:2], ["/opt/jsse", "--node"])
        self.assertEqual(node[1:], jsse[2:-1])
        self.assertEqual(jsse[-2:], ["tests", "--no-typecheck"])

    def test_listing_is_a_mocha_dry_run_of_the_bundle(self):
        argv = oracle.list_argv("jsse", "/opt/jsse")
        self.assertEqual(argv[:2], ["/opt/jsse", "--node"])
        self.assertIn("--dry-run", argv)
        self.assertEqual(argv[-1], os.path.join("built", "local", "run.js"))

    def test_both_variants_are_known(self):
        self.assertEqual(oracle.VARIANTS["typecheck"], [])
        self.assertEqual(oracle.VARIANTS["no-typecheck"], ["--no-typecheck"])
        self.assertIn("unittest", oracle.RUNNERS)
        self.assertIn("conformance", oracle.RUNNERS)


class ListingTest(unittest.TestCase):
    def test_identical_listings_do_not_differ(self):
        text = "TAP version 13\nok 1 a\nok 2 b\n1..2\n"
        self.assertEqual(oracle.compare_lists(text, text), [])

    def test_a_missing_renamed_or_extra_test_is_reported(self):
        ref = "ok 1 a\nok 2 b\n"
        self.assertEqual(len(oracle.compare_lists(ref, "ok 1 a\nok 2 c\n")), 1)
        self.assertIn("<missing>", oracle.compare_lists(ref, "ok 1 a\n")[0])
        self.assertIn("<missing>", oracle.compare_lists("ok 1 a\n", ref)[0])

    def test_the_report_is_capped(self):
        ref = "".join(f"ok {i} a\n" for i in range(100))
        other = "".join(f"ok {i} b\n" for i in range(100))
        lines = oracle.compare_lists(ref, other, limit=5)
        self.assertEqual(len(lines), 6)
        self.assertEqual(lines[-1], "...")

    def test_summary_counts_ok_lines_and_digests_the_whole_text(self):
        count, digest = oracle.list_summary("ok 1 a\nnot ok 2 b\nok 3 c\n")
        self.assertEqual(count, 2)
        self.assertNotEqual(digest, oracle.list_summary("ok 1 a\n")[1])


class BuildCompareTest(unittest.TestCase):
    ENGINES = [("node", "/usr/bin/node"), ("jsse", "/opt/jsse")]

    def tree(self, tmp, name, run_js):
        tree = os.path.join(tmp, name)
        os.makedirs(os.path.join(tree, "built", "local"))
        if run_js is not None:
            with open(os.path.join(tree, oracle.RUN_JS), "w") as f:
                f.write(run_js)
        return tree

    def results(self, jsse_code="0"):
        return {
            "node": ("0", "Completed tests in 1s\n", "", 1.0),
            "jsse": (jsse_code, "Completed tests in 9s\n", "", 9.0),
        }

    def test_identical_builds_have_no_problems(self):
        with tempfile.TemporaryDirectory() as tmp:
            trees = {e: self.tree(tmp, e, "bundle\n") for e in ("node", "jsse")}
            self.assertEqual(
                oracle.compare_builds("v", self.ENGINES, trees, self.results()), []
            )

    def test_a_different_bundle_is_reported(self):
        with tempfile.TemporaryDirectory() as tmp:
            trees = {
                "node": self.tree(tmp, "n", "one\n"),
                "jsse": self.tree(tmp, "j", "two\n"),
            }
            problems = oracle.compare_builds("v", self.ENGINES, trees, self.results())
            self.assertEqual(len(problems), 1)
            self.assertIn("differs: built/local/run.js", problems[0])

    def test_a_missing_bundle_and_a_failed_build_are_reported(self):
        with tempfile.TemporaryDirectory() as tmp:
            trees = {
                "node": self.tree(tmp, "n", "one\n"),
                "jsse": self.tree(tmp, "j", None),
            }
            problems = oracle.compare_builds("v", self.ENGINES, trees, self.results())
            self.assertTrue(any("was not built on jsse" in p for p in problems))
            problems = oracle.compare_builds(
                "v", self.ENGINES, trees, self.results(jsse_code="1")
            )
            self.assertTrue(any("hereby exited 1 on jsse" in p for p in problems))


class SkipTest(unittest.TestCase):
    def run_oracle(self, *args, ts_dir=None):
        env = {k: v for k, v in os.environ.items() if k != "TS_SRC_DIR"}
        if ts_dir is not None:
            env["TS_SRC_DIR"] = ts_dir
        return subprocess.run(
            [sys.executable, str(ORACLE), *args],
            env=env,
            capture_output=True,
            text=True,
        )

    def test_without_a_checkout_it_skips_unless_required(self):
        r = self.run_oracle()
        self.assertEqual(r.returncode, 0)
        self.assertIn("SKIP", r.stdout)
        r = self.run_oracle("--require")
        self.assertEqual(r.returncode, 1)
        self.assertIn("TS_SRC_DIR is not set", r.stderr)

    def test_a_directory_that_is_not_typescript_is_skipped(self):
        with tempfile.TemporaryDirectory() as tmp:
            r = self.run_oracle(ts_dir=tmp)
            self.assertEqual(r.returncode, 0)
            self.assertIn("no package.json", r.stdout)


if __name__ == "__main__":
    unittest.main()
