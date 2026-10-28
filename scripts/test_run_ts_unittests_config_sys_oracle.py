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
    res = {"outcomes": collections.Counter(), "errors": {}, "report": {}}
    for title, kind in outcomes:
        res["outcomes"][(title, kind)] += 1
        if kind == "failing":
            res["errors"][title] = "boom"
    return res


class ClassifyTest(unittest.TestCase):
    def test_equal_runs_have_nothing_to_report(self):
        a = run(("t1", "passing"), ("t2", "pending"))
        self.assertEqual(oracle.classify(a, a, set()), ([], [], [], [], []))

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
        regress, unexpected, _, node_fail, _ = oracle.classify(both, both, set())
        self.assertEqual((regress, node_fail), ([], ["t1"]))
        self.assertEqual(unexpected, [])

    def test_repeated_titles_count_as_a_multiset(self):
        node = run(("dup", "passing"), ("dup", "passing"))
        jsse = run(("dup", "passing"), ("dup", "failing"))
        self.assertEqual(oracle.classify(node, jsse, set())[0], ["dup"])
        self.assertEqual(
            oracle.counts(node), {"passing": 2, "failing": 0, "pending": 0}
        )


class DifferingTest(unittest.TestCase):
    def test_a_test_jsse_did_not_run_or_ran_twice_is_reported(self):
        node = run(("t1", "passing"), ("t2", "passing"), ("t3", "pending"))
        jsse = run(("t1", "passing"), ("t1", "passing"), ("t3", "passing"))
        self.assertEqual(
            oracle.classify(node, jsse, set()).differing, ["t1", "t2", "t3"]
        )

    def test_a_failure_is_not_also_reported_as_differing(self):
        node = run(("t1", "passing"))
        jsse = run(("t1", "failing"))
        self.assertEqual(oracle.classify(node, jsse, set()).differing, [])


class RecordTest(unittest.TestCase):
    def record(self, base, results, counts):
        with tempfile.TemporaryDirectory() as d:
            path = str(Path(d) / "b.json")
            code = oracle.record(path, base, counts, results)
            doc = None
            if code == 0:
                with open(path) as f:
                    doc = json.load(f)
        return code, doc

    def test_record_keeps_only_failures_that_still_fail_and_stays_valid_json(self):
        node = run(("unittests:: debugDeprecation a", "passing"))
        jsse = run(("unittests:: debugDeprecation a", "failing"))
        base = {
            "known_failures": {
                "unittests:: debugDeprecation a": "#9",
                "unittests:: debugDeprecation gone": "#8",
            }
        }
        code, doc = self.record(
            base,
            [("debugDeprecation", node, jsse)],
            {"debugDeprecation": oracle.counts(node)},
        )
        self.assertEqual(code, 0)
        self.assertEqual(
            doc["known_failures"], {"unittests:: debugDeprecation a": "#9"}
        )
        self.assertEqual(doc["counts"]["debugDeprecation"]["passing"], 1)

    def test_recording_a_subset_keeps_the_other_groups(self):
        node = run(("unittests:: debugDeprecation a", "passing"))
        base = {
            "counts": {
                "config-showConfig": {"passing": 148, "failing": 0, "pending": 0}
            },
            "known_failures": {"unittests:: config:: showConfig x": "#7"},
        }
        code, doc = self.record(
            base,
            [("debugDeprecation", node, node)],
            {"debugDeprecation": oracle.counts(node)},
        )
        self.assertEqual(code, 0)
        self.assertEqual(set(doc["counts"]), {"config-showConfig", "debugDeprecation"})
        self.assertEqual(doc["known_failures"], base["known_failures"])

    def test_record_refuses_a_missing_report_or_a_node_failure(self):
        node = dict(run(("t", "passing")), report=None)
        ok = dict(run(("t", "passing")), report={})
        self.assertEqual(self.record({}, [("debugDeprecation", node, ok)], {})[0], 1)
        failing = dict(run(("t", "failing")), report={})
        self.assertEqual(
            self.record({}, [("debugDeprecation", failing, failing)], {})[0], 1
        )


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
