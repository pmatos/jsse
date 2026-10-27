import importlib
import json
import os
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))

norm = importlib.import_module("tsc_oracle_norm")

ORACLE = SCRIPTS_DIR / "run-tsc-projects-oracle.py"
FIXTURES = SCRIPTS_DIR / "tsc-fixtures"

STATS_FAST = "Files:       7\nLines:      34\nMemory used: 13169K\nParse time: 0.01s\nTotal time: 0.06s\n"
STATS_SLOW = "Files:            7\nLines:           34\nMemory used: 206361K\nParse time:  0.13s\nTotal time:  0.48s\n"


class StatsTest(unittest.TestCase):
    def test_padding_times_and_memory_go_but_counts_stay(self):
        self.assertEqual(
            norm.normalise_stats(STATS_FAST), norm.normalise_stats(STATS_SLOW)
        )
        self.assertEqual(
            norm.normalise_stats(STATS_SLOW),
            "Files: 7\nLines: 34\nMemory used: <MEM>\nParse time: <TIME>\nTotal time: <TIME>\n",
        )

    def test_a_different_count_label_or_order_is_still_a_difference(self):
        base = norm.normalise_stats(STATS_FAST)
        self.assertNotEqual(base, norm.normalise_stats(STATS_FAST.replace("34", "35")))
        self.assertNotEqual(
            base, norm.normalise_stats(STATS_FAST.replace("Parse", "Bind"))
        )
        lines = STATS_FAST.splitlines(keepends=True)
        swapped = "".join([lines[1], lines[0], *lines[2:]])
        self.assertNotEqual(base, norm.normalise_stats(swapped))

    def test_other_lines_are_untouched(self):
        text = "error TS2322: Type 'a': 5 is bad\n  12: 3\n"
        self.assertEqual(norm.normalise_stats(text), text)


class TraceTest(unittest.TestCase):
    EVENTS = (
        {"name": "process_name", "ph": "M", "cat": "__metadata", "ts": 1.5},
        {
            "ph": "B",
            "cat": "program",
            "ts": 2,
            "name": "createProgram",
            "args": {"a": 1},
        },
        {"ph": "X", "cat": "check", "ts": 3, "dur": 4, "name": "slow"},
        {"ph": "E", "cat": "program", "ts": 5, "name": "createProgram"},
    )

    def render(self, events):
        return "[\n" + ",\n".join(json.dumps(e) for e in events) + "\n]\n"

    def test_timestamps_and_complete_events_go(self):
        out = norm.normalise_file("t/trace.json", self.render(self.EVENTS))
        names = [json.loads(line)["name"] for line in out.splitlines()]
        self.assertEqual(names, ["process_name", "createProgram", "createProgram"])
        self.assertNotIn("ts", out)
        self.assertIn('"a": 1', out)

    def test_begin_end_structure_and_arguments_are_compared(self):
        first = norm.normalise_file("trace.json", self.render(self.EVENTS))
        dropped = norm.normalise_file("trace.json", self.render(self.EVENTS[:2]))
        other = [self.EVENTS[0], dict(self.EVENTS[1], args={"a": 2}), *self.EVENTS[2:]]
        self.assertNotEqual(first, dropped)
        self.assertNotEqual(
            first, norm.normalise_file("trace.json", self.render(other))
        )

    def test_only_trace_files_and_a_broken_file_is_reported(self):
        self.assertEqual(norm.normalise_file("types.json", "[1]"), "[1]")
        self.assertTrue(
            norm.normalise_file("trace.json", "not json").startswith("<unparsable>")
        )


class ProfileTest(unittest.TestCase):
    def profile(self, extra_node=None, samples=(), deltas=()):
        frame = {
            "functionName": "(root)",
            "scriptId": "0",
            "url": "",
            "lineNumber": -1,
            "columnNumber": -1,
        }
        nodes = [
            {"id": 1, "callFrame": frame, "hitCount": 0, "children": [2]},
            {
                "id": 2,
                "callFrame": dict(frame, functionName="f"),
                "hitCount": 3,
                **(extra_node or {}),
            },
        ]
        return json.dumps(
            {
                "nodes": nodes,
                "startTime": 10,
                "endTime": 20,
                "samples": list(samples),
                "timeDeltas": list(deltas),
            }
        )

    def test_samples_and_optional_node_keys_do_not_matter(self):
        a = norm.normalise_file("p.cpuprofile", self.profile())
        b = norm.normalise_file(
            "p.cpuprofile", self.profile({"positionTicks": []}, [2, 2], [1, 1])
        )
        self.assertEqual(a, b)

    def test_a_malformed_profile_is_a_difference(self):
        good = norm.normalise_file("p.cpuprofile", self.profile())
        self.assertNotEqual(
            good, norm.normalise_file("p.cpuprofile", self.profile(samples=[2]))
        )
        broken = json.loads(self.profile())
        del broken["endTime"]
        self.assertNotEqual(
            good, norm.normalise_file("p.cpuprofile", json.dumps(broken))
        )


def write_exe(path, text):
    path.write_text(text)
    path.chmod(path.stat().st_mode | stat.S_IEXEC)


class StatsDirectiveTest(unittest.TestCase):
    """@stats normalises the statistics block of one invocation, and only that invocation."""

    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        root = Path(tmp.name)
        self.tsc = root / "typescript"
        (self.tsc / "lib").mkdir(parents=True)
        (self.tsc / "lib" / "_tsc.js").write_text("")
        (self.tsc / "package.json").write_text('{"version": "6.0.3"}')
        self.fix = root / "fix"
        (self.fix / "projects" / "p").mkdir(parents=True)
        (self.fix / "projects" / "p" / "a.ts").write_text("")
        (self.fix / "cases.txt").write_text(
            "stats | p | 0 0 | @stats x ;; @stats y\nraw | p | 0 | x\n"
        )
        self.bin = root / "bin"
        self.bin.mkdir()
        write_exe(self.bin / "node", self.script(STATS_FAST))
        write_exe(self.bin / "jsse", self.script(STATS_SLOW))

    def script(self, text):
        return "#!/bin/sh\ncat <<'EOF'\n" + text + "EOF\n"

    def run_oracle(self, *cases):
        env = {k: v for k, v in os.environ.items() if k != "TSC_DIR"}
        env.update(JSSE=str(self.bin / "jsse"), TSC_DIR=str(self.tsc))
        env["PATH"] = f"{self.bin}:{env['PATH']}"
        return subprocess.run(
            [
                sys.executable,
                "-I",
                str(ORACLE),
                "--fixtures",
                str(self.fix),
                "--jobs",
                "2",
                *cases,
            ],
            capture_output=True,
            text=True,
            env=env,
            check=False,
        )

    def test_statistics_with_other_padding_and_values_pass_with_the_directive(self):
        r = self.run_oracle("stats")
        self.assertEqual(r.returncode, 0, r.stdout)

    def test_without_the_directive_they_differ(self):
        r = self.run_oracle("raw")
        self.assertEqual(r.returncode, 1, r.stdout)

    def test_a_changed_count_still_fails_with_the_directive(self):
        write_exe(self.bin / "jsse", self.script(STATS_SLOW.replace("34", "35")))
        r = self.run_oracle("stats")
        self.assertEqual(r.returncode, 1, r.stdout)


class InfoCasesTest(unittest.TestCase):
    def cases(self):
        rows = []
        for line in (FIXTURES / "cases-info.txt").read_text().splitlines():
            if line.strip() and not line.startswith("#"):
                rows.append([p.strip() for p in line.split("|", 3)])
        return rows

    def test_cases_are_well_formed_and_pinned(self):
        names = set()
        for name, project, codes, args in self.cases():
            self.assertNotIn(name, names)
            names.add(name)
            self.assertTrue((FIXTURES / "projects" / project).is_dir(), name)
            self.assertEqual(len(codes.split()), len(args.split(" ;; ")), name)
        pinned = (FIXTURES / "expected-info.txt").read_text().split()
        self.assertEqual(set(pinned[0::2]), names)

    def test_every_informational_flag_has_a_case(self):
        text = (FIXTURES / "cases-info.txt").read_text()
        for flag in (
            "--listFiles ",
            "--listFilesOnly",
            "--listEmittedFiles",
            "--explainFiles",
            "--showConfig",
            "--init",
            "--traceResolution",
            "--extendedDiagnostics",
            "--diagnostics",
            "--generateTrace",
            "--generateCpuProfile",
        ):
            self.assertIn(flag, text)

    def test_moving_output_is_normalised_by_a_directive(self):
        for name, _, _, args in self.cases():
            if "Diagnostics" in args or "--diagnostics" in args:
                for invocation in args.split(" ;; "):
                    if "iagnostics" in invocation:
                        self.assertIn("@stats", invocation, name)


if __name__ == "__main__":
    unittest.main()
