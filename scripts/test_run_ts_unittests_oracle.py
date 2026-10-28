import collections
import importlib.util
import json
import os
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
ORACLE = SCRIPTS_DIR / "run-ts-unittests-oracle.py"

spec = importlib.util.spec_from_file_location("run_ts_unittests_oracle", ORACLE)
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)
groups = oracle.groups

C = collections.Counter
GROUP = groups.Group("semver", r"^unittests:: semver ", 2)
BATCH = [GROUP, groups.Group("paths", r"^unittests:: core paths ", 1)]


def report(passes=(), failures=(), pending=()):
    return C(passes), C(failures), C(pending)


class CompareTest(unittest.TestCase):
    def test_identical_runs_have_no_problems(self):
        both = report(["unittests:: semver a", "unittests:: semver b"])
        self.assertEqual(oracle.compare(GROUP, both, both, set()), [])

    def test_a_new_failure_is_reported(self):
        node = report(["unittests:: semver a", "unittests:: semver b"])
        jsse = report(["unittests:: semver a"], ["unittests:: semver b"])
        problems = oracle.compare(GROUP, node, jsse, set())
        self.assertEqual(
            problems,
            ["passes on Node, fails on jsse (unlisted): unittests:: semver b"],
        )

    def test_a_listed_failure_is_accepted(self):
        node = report(["unittests:: semver a", "unittests:: semver b"])
        jsse = report(["unittests:: semver a"], ["unittests:: semver b"])
        known = {"unittests:: semver b"}
        self.assertEqual(oracle.compare(GROUP, node, jsse, known), [])

    def test_a_fixed_failure_must_leave_the_list(self):
        both = report(["unittests:: semver a", "unittests:: semver b"])
        known = {"unittests:: semver b", "unittests:: core paths x"}
        problems = oracle.compare(GROUP, both, both, known)
        self.assertEqual(
            problems,
            ["listed as a known failure but no longer fails: unittests:: semver b"],
        )

    def test_a_test_that_did_not_run_is_a_difference(self):
        node = report(["unittests:: semver a", "unittests:: semver b"])
        jsse = report(["unittests:: semver a"])
        problems = oracle.compare(GROUP, node, jsse, set())
        self.assertEqual(problems, ["did not run on jsse: unittests:: semver b"])

    def test_node_must_pass_the_pinned_count(self):
        node = report(["unittests:: semver a"])
        problems = oracle.compare(GROUP, node, node, set())
        self.assertEqual(problems, ["Node passed 1 tests, the group pins 2"])

    def test_a_failure_on_node_is_reported(self):
        node = report(["unittests:: semver a"], ["unittests:: semver b"])
        problems = oracle.compare(GROUP, node, node, {"unittests:: semver b"})
        self.assertTrue(problems[0].startswith("Node itself fails or skips 1 tests"))

    def test_a_lost_duplicate_title_is_a_difference(self):
        node = report(["unittests:: semver a", "unittests:: semver a"])
        jsse = report(["unittests:: semver a"])
        problems = oracle.compare(GROUP, node, jsse, set())
        self.assertEqual(problems, ["did not run on jsse: unittests:: semver a"])

    def test_one_failing_duplicate_is_reported(self):
        node = report(["unittests:: semver a", "unittests:: semver a"])
        jsse = report(["unittests:: semver a"], ["unittests:: semver a"])
        problems = oracle.compare(GROUP, node, jsse, set())
        self.assertEqual(
            problems,
            ["passes on Node, fails on jsse (unlisted): unittests:: semver a"],
        )


class ReportTest(unittest.TestCase):
    def test_parse_report_reads_full_titles_with_multiplicity(self):
        text = (
            '{"passes": [{"fullTitle": "a b"}, {"fullTitle": "a b"}],'
            ' "failures": [{"fullTitle": "c"}], "pending": []}'
        )
        passes, failures, pending = oracle.parse_report(text)
        self.assertEqual(passes, C({"a b": 2}))
        self.assertEqual(failures, C({"c": 1}))
        self.assertEqual(pending, C())

    def test_split_keeps_one_groups_tests(self):
        parsed = report(
            ["unittests:: semver a", "unittests:: core paths x"],
            ["unittests:: semver b"],
        )
        passes, failures, _ = oracle.split(parsed, GROUP)
        self.assertEqual(passes, C({"unittests:: semver a": 1}))
        self.assertEqual(failures, C({"unittests:: semver b": 1}))

    def test_overlapping_patterns_are_found(self):
        wide = groups.Group("wide", r"^unittests:: ", 0)
        parsed = report(["unittests:: semver a", "unittests:: core paths x"])
        self.assertEqual(
            oracle.overlaps(parsed, [GROUP, wide]), ["unittests:: semver a"]
        )
        self.assertEqual(oracle.overlaps(parsed, BATCH), [])

    def test_known_failure_titles_may_hold_newlines(self):
        self.assertEqual(
            oracle.unescape("unittests:: x\\ny \\\\ z"), "unittests:: x\ny \\ z"
        )

    def test_annotations_escape_the_characters_that_split_them(self):
        self.assertEqual(
            oracle.annotation("a 100%\nb"),
            "::error title=unit-test oracle::a 100%25%0Ab",
        )

    def test_known_failures_ignore_comments(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d, "known.txt")
            path.write_text("# comment\n\nunittests:: semver b\n")
            self.assertEqual(
                oracle.read_known_failures(str(path)), {"unittests:: semver b"}
            )
        self.assertEqual(oracle.read_known_failures("/nonexistent"), set())

    def test_the_committed_known_failures_name_a_group(self):
        for title in oracle.read_known_failures():
            hits = [g for g in groups.GROUPS if re.search(g.pattern, title)]
            self.assertEqual(len(hits), 1, title)


class GroupsTest(unittest.TestCase):
    def test_batches_partition_the_groups(self):
        batches = oracle.batches_of(groups.GROUPS)
        flat = [g.name for b in batches for g in b]
        self.assertEqual(sorted(flat), sorted(g.name for g in groups.GROUPS))
        self.assertEqual(len({g.name for g in groups.GROUPS}), len(groups.GROUPS))

    def test_check_groups_covers_the_services_directories(self):
        with tempfile.TemporaryDirectory() as d:
            base = Path(d, "src", "testRunner", "unittests", "services")
            (base / "extract").mkdir(parents=True)
            (base.parent / "tsbuild").mkdir()
            Path(d, "src", "testRunner", "unittests", "evaluation").mkdir()
            (base / "textChanges.ts").write_text(
                'describe("unittests:: services:: textChanges", () => {});\n'
            )
            (base / "extract" / "ranges.ts").write_text(
                'describe("unittests:: services:: extract:: extractRanges", () => {});\n'
                'describe("unittests:: services:: extract:: brandNew", () => {});\n'
            )
            (base / "extract" / "deeper").mkdir()
            (base / "extract" / "deeper" / "new.ts").write_text(
                'describe("unittests:: services:: deeper", () => {});\n'
            )
            self.assertEqual(
                groups.check_groups(d),
                [
                    "services/extract/ranges.ts: describe("
                    "'unittests:: services:: extract:: brandNew') matches groups none",
                    "services/extract/deeper/new.ts: describe("
                    "'unittests:: services:: deeper') matches groups none",
                ],
            )

    def test_check_groups_flags_a_missing_owned_directory(self):
        with tempfile.TemporaryDirectory() as d:
            Path(d, "src", "testRunner", "unittests").mkdir(parents=True)
            self.assertEqual(
                groups.check_groups(d),
                [
                    "unittests/services is missing: its groups select nothing",
                    "unittests/evaluation is missing: its groups select nothing",
                    "unittests/tsbuild is missing: its groups select nothing",
                ],
            )

    def test_check_groups_covers_the_tsbuild_directory(self):
        with tempfile.TemporaryDirectory() as d:
            base = Path(d, "src", "testRunner", "unittests")
            (base / "services").mkdir(parents=True)
            (base / "evaluation").mkdir()
            (base / "tsbuild").mkdir()
            (base / "tsbuild" / "sample.ts").write_text(
                "describe(\"unittests:: tsbuild:: on 'sample1' project\", () => {});\n"
                '    describe("unittests:: tsbuild:: nested", () => {});\n'
            )
            (base / "tsbuild" / "watch.ts").write_text(
                'describe("unittests:: tsbuildWatch:: on sample", () => {});\n'
                'describe("unittests:: tsbuild:: brandNew:: x", () => {});\n'
            )
            self.assertEqual(
                groups.check_groups(d),
                [
                    "tsbuild/watch.ts: describe('unittests:: tsbuildWatch:: on sample') "
                    "matches groups none",
                    "tsbuild/watch.ts: describe('unittests:: tsbuild:: brandNew:: x') "
                    "matches groups none",
                ],
            )

    def test_check_groups_covers_the_evaluation_directory(self):
        with tempfile.TemporaryDirectory() as d:
            base = Path(d, "src", "testRunner", "unittests")
            Path(base, "services").mkdir(parents=True)
            Path(base, "tsbuild").mkdir()
            Path(base, "evaluation").mkdir()
            (base / "evaluation" / "forOf.ts").write_text(
                'describe("unittests:: evaluation:: forOfEvaluation", () => {});\n'
                'describe("unittests:: evaluation:: brandNew", () => {});\n'
            )
            self.assertEqual(
                groups.check_groups(d),
                [
                    "evaluation/forOf.ts: describe("
                    "'unittests:: evaluation:: brandNew') matches groups none"
                ],
            )

    def assert_counts_equal_the_pinned_suite(self, suite):
        pins = json.loads(Path(oracle.ROOT, "ts-suite", "oracle.json").read_text())
        pinned = pins["runners"]["unittests"]["suites"][suite]["passed"]
        got = sum(g.tests for g in groups.GROUPS if g.name.startswith(suite + "-"))
        self.assertEqual(got, pinned, suite)

    def test_the_evaluation_counts_equal_the_pinned_suite_count(self):
        self.assert_counts_equal_the_pinned_suite("evaluation")

    def test_the_services_counts_equal_the_pinned_suite_count(self):
        self.assert_counts_equal_the_pinned_suite("services")

    def test_the_tsbuild_counts_equal_the_pinned_suite_count(self):
        self.assert_counts_equal_the_pinned_suite("tsbuild")

    def test_tsbuild_patterns_leave_tsbuild_watch_and_outfile_alone(self):
        def hit(title):
            return [g.name for g in groups.GROUPS if re.search(g.pattern, title)]

        self.assertEqual(
            hit("unittests:: tsbuild:: outFile:: amdModulesWithOut:: x y"),
            ["tsbuild-amdModulesWithOut"],
        )
        self.assertEqual(
            hit("unittests:: tsbuild:: outFile:: x y"), ["tsbuild-outFile"]
        )
        self.assertEqual(hit("unittests:: tsbuildWatch:: on sample x"), [])

    def test_check_groups_flags_an_unmatched_suite(self):
        with tempfile.TemporaryDirectory() as d:
            base = Path(d, "src", "testRunner", "unittests")
            base.mkdir(parents=True)
            (base / "new.ts").write_text(
                'describe("unittests:: brandNew", () => {});\n'
            )
            (base / "asserts.ts").write_text(
                'describe("unittests:: assert", () => {});\n'
            )
            (base / "quoted.ts").write_text(
                "describe(\n    'unittests:: semver',\n    () => {},\n);\n"
            )
            (base / "skipped.ts").write_text(
                "describe.skip(`unittests:: qux`, () => {});\n"
            )
            (base / "nested.ts").write_text(
                '    describe("unittests:: indented", () => {});\n'
            )
            Path(base, "services").mkdir()
            Path(base, "evaluation").mkdir()
            Path(base, "tsbuild").mkdir()
            problems = groups.check_groups(d)
        self.assertEqual(
            problems,
            [
                "new.ts: describe('unittests:: brandNew') matches groups none",
                "skipped.ts: describe('unittests:: qux') matches groups none",
            ],
        )

    def test_every_pattern_is_anchored(self):
        for g in groups.GROUPS:
            self.assertEqual(g.pattern[0], "^", g.name)


class CommandLineTest(unittest.TestCase):
    def run_oracle(self, env_extra, *args):
        env = {k: v for k, v in os.environ.items() if k != "TS_SRC_DIR"}
        env.update(env_extra)
        return subprocess.run(
            [sys.executable, str(ORACLE), *args],
            capture_output=True,
            text=True,
            env=env,
        )

    def test_skips_without_a_checkout(self):
        r = self.run_oracle({})
        self.assertEqual(r.returncode, 0)
        self.assertIn("SKIP: TS_SRC_DIR is not set", r.stdout)

    def test_require_makes_the_skip_an_error(self):
        r = self.run_oracle({}, "--require")
        self.assertEqual(r.returncode, 3)

    def test_list_prints_group_names(self):
        r = self.run_oracle({}, "--list", "semver", "pat*")
        self.assertEqual(r.stdout.split(), ["paths", "semver"])

    def test_unknown_group_is_an_error(self):
        r = self.run_oracle({}, "--list", "nope")
        self.assertNotEqual(r.returncode, 0)


if __name__ == "__main__":
    unittest.main()
