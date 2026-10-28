import importlib.util
import json
import re
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
ORACLE = SCRIPTS_DIR / "run-ts-unittests-tsc-oracle.py"

spec = importlib.util.spec_from_file_location("run_ts_unittests_tsc", ORACLE)
wrapper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wrapper)
oracle = wrapper.base


class WrapperTest(unittest.TestCase):
    def test_the_base_oracle_runs_the_tsc_groups_and_baseline(self):
        self.assertEqual(list(oracle.GROUPS)[0], "tsc-cancellationToken")
        self.assertTrue(oracle.BASELINE.endswith("ts-suite/unittests-tsc.json"))
        self.assertEqual(oracle.SUITE_OF, {"tsc": "tsc-"})

    def test_each_title_belongs_to_exactly_one_group(self):
        titles = (
            "unittests:: tsc:: builder cancellationToken:: x",
            "unittests:: tsc:: libraryResolution:: library file resolution x",
            "unittests:: tsc:: projectReferencesConfig:: project-references meta check x",
            "unittests:: tsc:: noEmit:: x",
            "unittests:: tsc:: noEmitOnError:: x",
        )
        for title in titles:
            hits = [n for n, rx in oracle.GROUPS.items() if re.search(rx, title)]
            self.assertEqual(len(hits), 1, (title, hits))

    def test_tscWatch_and_tsbuild_titles_belong_to_no_group(self):
        for title in (
            "unittests:: tscWatch:: noEmit:: x",
            "unittests:: tsbuild:: x",
            "unittests:: tsbuildWatch:: x",
        ):
            self.assertFalse(any(re.search(rx, title) for rx in oracle.GROUPS.values()))

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
