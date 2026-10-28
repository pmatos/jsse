import collections
import importlib.util
import json
import re
import tempfile
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
ORACLE = SCRIPTS_DIR / "run-ts-unittests-config-sys-oracle.py"

spec = importlib.util.spec_from_file_location("run_ts_unittests_oracle", ORACLE)
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)


def run(*outcomes):
    res = {"outcomes": collections.Counter(), "errors": {}}
    for title, kind in outcomes:
        res["outcomes"][(title, kind)] += 1
        if kind == "failing":
            res["errors"][title] = "boom"
    return res


class ClassifyTest(unittest.TestCase):
    def test_equal_runs_have_nothing_to_report(self):
        a = run(("t1", "passing"), ("t2", "pending"))
        self.assertEqual(oracle.classify(a, a, set()), ([], [], [], []))

    def test_node_pass_and_jsse_fail_is_unexpected_unless_listed(self):
        node = run(("t1", "passing"), ("t2", "passing"))
        jsse = run(("t1", "passing"), ("t2", "failing"))
        self.assertEqual(oracle.classify(node, jsse, set())[:2], (["t2"], ["t2"]))
        self.assertEqual(oracle.classify(node, jsse, {"t2"})[:3], (["t2"], [], []))

    def test_listed_test_that_now_passes_is_stale(self):
        node = run(("t1", "passing"))
        self.assertEqual(oracle.classify(node, node, {"t1"})[2], ["t1"])

    def test_failure_on_both_engines_is_reported_as_a_node_failure(self):
        both = run(("t1", "failing"))
        regress, unexpected, _, node_fail = oracle.classify(both, both, set())
        self.assertEqual((regress, node_fail), ([], ["t1"]))
        self.assertEqual(unexpected, [])

    def test_repeated_titles_count_as_a_multiset(self):
        node = run(("dup", "passing"), ("dup", "passing"))
        jsse = run(("dup", "passing"), ("dup", "failing"))
        self.assertEqual(oracle.classify(node, jsse, set())[0], ["dup"])
        self.assertEqual(
            oracle.counts(node), {"passing": 2, "failing": 0, "pending": 0}
        )


class RecordTest(unittest.TestCase):
    def test_record_keeps_only_failures_that_still_fail_and_stays_valid_json(self):
        node = run(("t1", "passing"), ("t2", "passing"))
        jsse = run(("t1", "passing"), ("t2", "failing"))
        base = {"known_failures": {"t2": "#9", "gone": "#8"}}
        with tempfile.TemporaryDirectory() as d:
            path = str(Path(d) / "b.json")
            oracle.record(path, base, {"g": oracle.counts(node)}, [("g", node, jsse)])
            with open(path) as f:
                doc = json.load(f)
        self.assertEqual(doc["known_failures"], {"t2": "#9"})
        self.assertEqual(doc["counts"]["g"]["passing"], 2)


class GroupsTest(unittest.TestCase):
    @staticmethod
    def baseline():
        with open(oracle.BASELINE) as f:
            return json.load(f)

    def test_each_title_belongs_to_exactly_one_group(self):
        titles = (
            "unittests:: config:: matchFiles with literal file list",
            "unittests:: config:: tsconfigParsingWatchOptions:: parseConfigFileTextToJson x",
            "unittests:: sys:: symlinkWatching:: with ts.sys:: x",
            "unittests:: debugDeprecation deprecateFunction x",
        )
        for title in titles:
            hits = [n for n, rx in oracle.GROUPS.items() if re.search(rx, title)]
            self.assertEqual(len(hits), 1, (title, hits))

    def test_committed_baseline_has_a_count_per_group_and_no_untracked_failure(self):
        base = self.baseline()
        self.assertEqual(set(base["counts"]), set(oracle.GROUPS))
        for why in base["known_failures"].values():
            self.assertRegex(why, r"#\d+")

    def test_counts_match_the_pinned_node_oracle(self):
        base = self.baseline()
        self.assertEqual(oracle.pinned_suite_problems(base["counts"]), [])


if __name__ == "__main__":
    unittest.main()
