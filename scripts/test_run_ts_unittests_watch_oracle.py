import contextlib
import importlib.util
import io
import json
import re
import sys
import unittest
from pathlib import Path
from unittest import mock

SCRIPTS_DIR = Path(__file__).resolve().parent
ORACLE = SCRIPTS_DIR / "run-ts-unittests-watch-oracle.py"

spec = importlib.util.spec_from_file_location("run_ts_unittests_watch", ORACLE)
wrapper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wrapper)
oracle = wrapper.base


def hits(title):
    return [n for n, rx in oracle.GROUPS.items() if re.search(rx, title)]


class WrapperTest(unittest.TestCase):
    def test_the_base_oracle_runs_the_watch_groups_and_baseline(self):
        self.assertEqual(list(oracle.GROUPS)[0], "tscWatch-consoleClearing")
        self.assertTrue(oracle.BASELINE.endswith("ts-suite/unittests-watch.json"))
        self.assertEqual(
            oracle.SUITE_OF, {"tscWatch": "tscWatch-", "tsbuildWatch": "tsbuildWatch-"}
        )

    def test_main_lists_the_watch_groups(self):
        out = io.StringIO()
        with mock.patch.object(sys, "argv", ["oracle", "--list"]):
            with contextlib.redirect_stdout(out):
                self.assertEqual(oracle.main(), 0)
        self.assertEqual(out.getvalue().split(), list(oracle.GROUPS))

    def test_each_title_belongs_to_exactly_one_group(self):
        titles = (
            "unittests:: tscWatch:: emit:: with outFile or out setting x",
            "unittests:: tscWatch:: emitAndErrorUpdates:: Emit times x",
            "unittests:: tscWatch:: noEmit:: x",
            "unittests:: tscWatch:: noEmitOnError:: x",
            "unittests:: tscWatch:: watchAPI:: with sourceOfProjectReferenceRedirect:: x",
            "unittests:: tscWatch:: watchAPI:: tsc-watch with custom module resolution x",
            "unittests:: tscWatch:: watchEnvironment:: tsc-watch x",
            "unittests:: tscWatch:: resolveJsonModule:: emit file --incremental x",
            "unittests:: tsbuildWatch:: watchMode:: with noEmit:: x",
            "unittests:: tsbuildWatch:: watchMode:: with noEmitOnError:: x",
            "unittests:: tsbuildWatch:: watchMode:: publicAPI:: Public API x",
            "unittests:: tsbuildWatch:: watchEnvironment:: tsbuild:: watchMode:: x",
        )
        for title in titles:
            self.assertEqual(len(hits(title)), 1, (title, hits(title)))

    def test_other_unittest_suites_belong_to_no_group(self):
        for title in (
            "unittests:: tsc:: noEmit:: x",
            "unittests:: tsbuild:: x",
            "unittests:: tsserver:: x",
        ):
            self.assertEqual(hits(title), [])

    def test_committed_baseline_has_a_count_per_group_and_issue_numbers(self):
        with open(oracle.BASELINE) as f:
            base = json.load(f)
        self.assertEqual(set(base["counts"]), set(oracle.GROUPS))
        for why in base["known_failures"].values():
            self.assertRegex(why, r"#\d+")

    def test_counts_match_the_pinned_node_oracle(self):
        with open(oracle.BASELINE) as f:
            base = json.load(f)
        self.assertEqual(oracle.pinned_suite_problems(base["counts"]), [])


if __name__ == "__main__":
    unittest.main()
