"""Guards for the second informational-flag case set of the tsc project oracle (#1247):
scripts/tsc-fixtures/cases-info-more.txt over projects/info-more, and the normalisers it added to
scripts/tsc_oracle_norm.py. Run with `python3 -m unittest scripts/test_tsc_oracle_norm_1247.py`."""

import importlib
import json
import sys
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))

norm = importlib.import_module("tsc_oracle_norm")

FIXTURES = SCRIPTS_DIR / "tsc-fixtures"
HARNESS_STEPS = ("@utimes", "@replace", "@remove", "@mtimes-after")


def info_more_cases():
    rows = []
    for line in (FIXTURES / "cases-info-more.txt").read_text().splitlines():
        if line.strip() and not line.startswith("#"):
            rows.append([p.strip() for p in line.split("|", 3)])
    return rows


class NormaliserTest(unittest.TestCase):
    def test_a_hyphenated_label_has_its_padding_and_time_normalised(self):
        fast = "Build time:   0.50s\nUp-to-date check time:   0.00s\n"
        slow = "Build time:   0.50s\nUp-to-date check time:    0.00s\n"
        self.assertEqual(norm.normalise_stats(fast), norm.normalise_stats(slow))
        self.assertIn("Up-to-date check time: <TIME>", norm.normalise_stats(fast))

    def test_a_changed_hyphenated_count_is_still_a_difference(self):
        a = "Up-to-date checks: 3\n"
        self.assertNotEqual(
            norm.normalise_stats(a), norm.normalise_stats("Up-to-date checks: 4\n")
        )

    def test_build_trace_names_lose_the_process_id(self):
        self.assertEqual(
            norm.normalise_name("tr/trace.2295619-1.json"), "tr/trace.<PID>-1.json"
        )
        self.assertEqual(
            norm.normalise_name("tr/types.7-12.json"), "tr/types.<PID>-12.json"
        )
        for same in ("tr/trace.json", "tr/types.json", "src/trace.1.txt", "a.json"):
            self.assertEqual(norm.normalise_name(same), same)

    def test_the_project_counter_of_a_build_trace_name_is_compared(self):
        self.assertNotEqual(
            norm.normalise_name("trace.5-1.json"), norm.normalise_name("trace.5-2.json")
        )

    def test_legend_json_paths_lose_the_process_id_too(self):
        text = '[{"tracePath":"tr/trace.99-1.json","typesPath":"tr/types.99-1.json"}]'
        out = norm.normalise_file("tr/legend.json", text)
        self.assertEqual(
            out,
            '[{"tracePath":"tr/trace.<PID>-1.json","typesPath":"tr/types.<PID>-1.json"}]',
        )
        self.assertEqual(norm.normalise_file("other.json", text), text)

    def test_a_negative_time_delta_is_not_a_difference_but_a_non_number_is(self):
        frame = {
            "functionName": "(root)",
            "scriptId": "0",
            "url": "",
            "lineNumber": -1,
            "columnNumber": -1,
        }

        def profile(deltas):
            return json.dumps(
                {
                    "nodes": [{"id": 1, "callFrame": frame, "hitCount": 0}],
                    "startTime": 1,
                    "endTime": 2,
                    "samples": [1] * len(deltas),
                    "timeDeltas": deltas,
                }
            )

        a = norm.normalise_file("p.cpuprofile", profile([3, -1]))
        self.assertEqual(a, norm.normalise_file("p.cpuprofile", profile([3, 5])))
        self.assertNotEqual(a, norm.normalise_file("p.cpuprofile", profile([3, "5"])))


class InfoMoreCasesTest(unittest.TestCase):
    def test_cases_are_well_formed_and_pinned(self):
        names = set()
        for name, project, codes, args in info_more_cases():
            self.assertTrue(name.startswith("infm-"), name)
            self.assertNotIn(name, names)
            names.add(name)
            self.assertTrue((FIXTURES / "projects" / project).is_dir(), name)
            runs = [a for a in args.split(" ;; ") if a.split()[0] not in HARNESS_STEPS]
            self.assertEqual(len(codes.split()), len(runs), name)
        pinned = (FIXTURES / "expected-info-more.txt").read_text().split()
        self.assertEqual(set(pinned[0::2]), names)
        self.assertEqual(len(pinned) // 2, len(names))

    def test_every_informational_flag_is_exercised_in_build_mode_and_with_references(
        self,
    ):
        text = (FIXTURES / "cases-info-more.txt").read_text()
        for flag in (
            "-b --listFiles",
            "-b --listEmittedFiles",
            "-b --explainFiles",
            "-b --extendedDiagnostics",
            "-b --traceResolution",
            "-b --generateTrace",
            "-b --generateCpuProfile",
            "-b --showConfig",
            "tsconfig.refs.json --explainFiles",
            "--locale de --explainFiles",
            "--generateCpuProfile tsconfig.json/p.cpuprofile",
            "@args.txt",
        ):
            self.assertIn(flag, text)

    def test_moving_statistics_are_normalised_by_a_directive(self):
        for name, _, _, args in info_more_cases():
            for invocation in args.split(" ;; "):
                if "iagnostics" in invocation and "--init" not in invocation:
                    self.assertTrue(invocation.startswith("@stats"), name)

    def test_the_known_differences_file_lists_issues_only(self):
        rows = [
            line.split("\t")
            for line in (FIXTURES / "known-differences-1247.txt")
            .read_text()
            .splitlines()
            if line.strip() and not line.startswith("#")
        ]
        self.assertTrue(rows)
        for row in rows:
            self.assertEqual(len(row), 3, row)
            self.assertRegex(row[1], r"^#\d+$")

    def test_the_fixture_tree_has_what_the_cases_reference(self):
        tree = FIXTURES / "projects" / "info-more"
        for rel in (
            "tsconfig.json",
            "tsconfig.js.json",
            "tsconfig.comp.json",
            "tsconfig.refs.json",
            "tsconfig.outfile.json",
            "tsconfig.paths.json",
            "tsconfig.uni.json",
            "sub/tsconfig.json",
            "node_modules/shared-config/tsconfig.json",
            "args.txt",
            "args-nested.txt",
            "args-showconfig.txt",
            "typeroots/special/index.d.ts",
        ):
            self.assertTrue((tree / rel).is_file(), rel)


if __name__ == "__main__":
    unittest.main()
