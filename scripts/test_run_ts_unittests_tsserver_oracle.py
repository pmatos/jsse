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
ORACLE = SCRIPTS_DIR / "run-ts-unittests-tsserver-oracle.py"

spec = importlib.util.spec_from_file_location("run_ts_unittests_tsserver", ORACLE)
wrapper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wrapper)
oracle = wrapper.base
table = wrapper.table


def listed(*argv):
    out = io.StringIO()
    with mock.patch.object(sys, "argv", ["oracle"]):
        with contextlib.redirect_stdout(out):
            code = wrapper.main([*argv, "--list"])
    return code, out.getvalue().split()


class WrapperTest(unittest.TestCase):
    def test_the_base_oracle_runs_the_tsserver_groups_and_baseline(self):
        self.assertEqual(list(oracle.GROUPS), list(table.GROUPS))
        self.assertTrue(oracle.BASELINE.endswith("ts-suite/unittests-tsserver.json"))
        self.assertEqual(oracle.SUITE_OF, {"tsserver": "tsserver-"})

    def test_half_selects_the_default_groups(self):
        code, names = listed()
        self.assertEqual((code, names), (0, table.groups_of_half("a-m")))
        self.assertEqual(listed("--half", "a-m")[1], names)
        n_z = listed("--half=n-z")[1]
        self.assertEqual(listed("--half", "all")[1], list(table.GROUPS))
        self.assertEqual(sorted(names + n_z), sorted(table.GROUPS))
        self.assertFalse(set(names) & set(n_z))

    def test_a_group_named_on_the_command_line_runs_whatever_the_half(self):
        self.assertEqual(
            listed("tsserver-typingsInstaller")[1], ["tsserver-typingsInstaller"]
        )

    def test_an_unknown_half_exits(self):
        with self.assertRaises(SystemExit):
            wrapper.main(["--half", "q-z"])

    def test_every_file_is_in_one_batch_and_batches_do_not_straddle_halves(self):
        self.assertEqual(table.table_problems(), [])
        files = [f for fs in table.BATCHES.values() for f in fs]
        self.assertEqual(sorted(files), sorted(table.FILES))

    def test_halves_go_by_the_first_letter_of_the_file_name(self):
        self.assertEqual(table.half_of("moduleSpecifierCache"), "a-m")
        self.assertEqual(table.half_of("navTo"), "n-z")
        self.assertEqual(table.half_of("events/watchEvents"), "a-m")
        self.assertEqual(table.half_of("symLinks"), "n-z")

    def test_each_title_belongs_to_exactly_one_group(self):
        titles = (
            "unittests:: tsserver:: completions:: x",
            "unittests:: tsserver:: completionsIncomplete:: x",
            "unittests:: tsserver:: plugins:: loading x",
            "unittests:: tsserver:: pluginsAsync:: async loaded plugins x",
            "unittests:: tsserver:: events:: projectLoading:: x",
            "unittests:: tsserver:: services:: goToDefinition:: x",
            "unittests:: tsserver:: with projectReferences:: and tsbuild x",
            "unittests:: tsserver:: projectReferencesSourcemap:: x",
            "unittests:: tsserver:: projectsWithReferences:: x",
            "unittests:: tsserver:: projects:: x",
            "unittests:: tsserver:: reload:: x",
            "unittests:: tsserver:: reloadProjects:: x",
            "unittests:: tsserver:: symLinks:: x",
            "unittests:: tsserver:: symlinkCache:: x",
            "unittests:: tsserver:: textStorage should switch to script version cache",
            "unittests:: tsserver:: VersionCache TS code handles empty lines array",
            "unittests:: tsserver:: Session:: General functionality x",
        )
        for title in titles:
            hits = [n for n, rx in oracle.GROUPS.items() if re.search(rx, title)]
            self.assertEqual(len(hits), 1, (title, hits))

    def test_other_suites_belong_to_no_group(self):
        for title in (
            "unittests:: tscWatch:: x",
            "unittests:: services:: x",
            "unittests:: tsbuild:: x",
        ):
            self.assertFalse(any(re.search(rx, title) for rx in oracle.GROUPS.values()))

    def test_committed_baseline_has_a_count_per_group_and_issue_numbers(self):
        with open(oracle.BASELINE) as f:
            base = json.load(f)
        self.assertEqual(set(base["counts"]), set(oracle.GROUPS))
        for why in base["known_failures"].values():
            self.assertRegex(why, r"#\d+")

    def test_counts_of_both_halves_equal_the_pinned_node_oracle(self):
        self.assertEqual(oracle.pinned_suite_problems({}), [])

    def test_a_changed_count_breaks_the_pin(self):
        one_less = {"passing": 1, "failing": 0, "pending": 0}
        self.assertEqual(
            len(oracle.pinned_suite_problems({"tsserver-typingsInstaller": one_less})),
            1,
        )


if __name__ == "__main__":
    unittest.main()
