import os
import shlex
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_run_tsc_projects_oracle import write_exe

SCRIPTS_DIR = Path(__file__).resolve().parent
ORACLE = SCRIPTS_DIR / "run-tsc-watch-oracle.py"
FIXTURES = SCRIPTS_DIR / "tsc-fixtures"

WATCHER = """#!/usr/bin/env python3
import os, sys, time
import signal

def tree():
    out = {}
    for d, _, names in os.walk("."):
        for n in names:
            with open(os.path.join(d, n), "rb") as f:
                out[os.path.join(d, n)] = f.read()
    return out

signal.signal(signal.SIGINT, signal.SIG_DFL)
print("12:01:02 AM - Starting compilation in watch mode...", flush=True)
print("12:01:02 AM - Found 0 errors. Watching for file changes.", flush=True)
seen = tree()
while True:
    time.sleep(0.05)
    now = tree()
    if now != seen:
        stable = 0
        while stable < 4:
            time.sleep(0.05)
            again = tree()
            stable = stable + 1 if again == now else 0
            now = again
        seen = now
        print("12:01:03 PM - File change detected. Starting incremental compilation...", flush=True)
        {extra}
        print("12:01:03 PM - Found 0 errors. Watching for file changes.", flush=True)
"""

EXITING = """#!/bin/sh
echo "12:01:02 AM - Starting compilation in watch mode..."
echo "12:01:02 AM - Found 0 errors. Watching for file changes."
sleep 0.3
echo "error TS5083: Cannot read file '$(pwd)/tsconfig.json'." >&2
exit 1
"""


def watcher(extra="pass"):
    return WATCHER.replace("{extra}", extra)


class WatchOracleTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        root = Path(self.tmp.name)
        self.tsc = root / "typescript"
        (self.tsc / "lib").mkdir(parents=True)
        (self.tsc / "lib" / "_tsc.js").write_text("")
        (self.tsc / "package.json").write_text('{"version": "6.0.3"}')
        self.fix = root / "fix"
        (self.fix / "projects" / "p").mkdir(parents=True)
        (self.fix / "projects" / "p" / "a.ts").write_text("let one = 1;")
        (self.fix / "projects" / "p" / "gone.ts").write_text("")
        self.bin = root / "bin"
        self.bin.mkdir()
        write_exe(self.bin / "node", watcher())
        write_exe(root / "jsse", watcher())

    def cases(self, text):
        (self.fix / "watch-cases.txt").write_text(text)

    def run_oracle(self, *extra, tsc_dir=True):
        env = {k: v for k, v in os.environ.items() if k != "TSC_DIR"}
        env["PATH"] = f"{self.bin}:{env['PATH']}"
        env["JSSE"] = str(Path(self.tmp.name) / "jsse")
        if tsc_dir:
            env["TSC_DIR"] = str(self.tsc)
        return subprocess.run(
            [
                sys.executable,
                "-I",
                str(ORACLE),
                "--fixtures",
                str(self.fix),
                "--settle",
                "0.3",
                *extra,
            ],
            check=False,
            capture_output=True,
            text=True,
            env=env,
            timeout=120,
        )

    def test_every_edit_kind_is_applied_and_equal_sessions_pass(self):
        self.cases(
            "all | p | -w -p . | @replace a.ts one two ;; @wait ;; "
            '@write b.ts "x\\n" ;; @append b.ts y ;; @mkdir d ;; @rename b.ts d/c.ts ;; '
            "@touch a.ts ;; @remove gone.ts ;; @wait ;; @idle 0.2\n"
        )
        r = self.run_oracle("--ci", "--dump", str(Path(self.tmp.name) / "dump"))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("tsc watch oracle: 1/1 passed", r.stdout)
        record = (Path(self.tmp.name) / "dump" / "all.jsse.txt").read_text()
        self.assertIn("=== initial compile", record)
        self.assertIn("--- file d/c.ts\nx\ny", record)
        self.assertIn("--- file gone.ts\n<deleted>", record)
        self.assertIn("<TIME> - File change detected", record)
        self.assertIn("(exit -2)", record)

    def test_a_difference_in_the_sequence_of_cycles_fails(self):
        self.cases("diff | p | -w -p . | @append a.ts x ;; @wait\n")
        write_exe(
            Path(self.tmp.name) / "jsse",
            watcher('print("a.ts(1,1): error TS1: extra", flush=True)'),
        )
        r = self.run_oracle("--ci")
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("+a.ts(1,1): error TS1: extra", r.stdout)

    def test_an_unexpected_early_exit_fails_and_exits_marker_allows_it(self):
        write_exe(self.bin / "node", EXITING)
        write_exe(Path(self.tmp.name) / "jsse", EXITING)
        self.cases("early | p | -w -p . | @remove a.ts ;; @wait\n")
        self.assertEqual(self.run_oracle("--ci").returncode, 1)
        self.cases("early | p | -w -p . | @exits ;; @remove a.ts ;; @wait\n")
        r = self.run_oracle("--ci")
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)

    def test_exits_marker_fails_a_session_that_keeps_running(self):
        self.cases("alive | p | -w -p . | @exits ;; @append a.ts x ;; @wait\n")
        r = self.run_oracle("--ci")
        self.assertEqual(r.returncode, 1, r.stdout)

    def test_pin_and_no_node_round_trip(self):
        self.cases("pinned | p | -w -p . | @append a.ts x ;; @wait\n")
        self.assertEqual(self.run_oracle("--ci", "--pin").returncode, 0)
        self.assertEqual(self.run_oracle("--ci", "--no-node").returncode, 0)
        write_exe(
            Path(self.tmp.name) / "jsse",
            watcher('print("changed", flush=True)'),
        )
        self.assertEqual(self.run_oracle("--ci", "--no-node").returncode, 1)

    def test_unknown_steps_and_cases_abort_and_a_missing_tsc_dir_skips(self):
        self.cases("bad | p | -w -p . | @nope x\n")
        self.assertNotEqual(self.run_oracle().returncode, 0)
        self.assertNotEqual(self.run_oracle("missing").returncode, 0)
        r = self.run_oracle(tsc_dir=False)
        self.assertEqual(r.returncode, 0)
        self.assertIn("SKIP: TSC_DIR is not set", r.stdout)
        self.assertEqual(self.run_oracle("--require", tsc_dir=False).returncode, 3)


class FixtureConsistencyTest(unittest.TestCase):
    def test_cases_projects_and_pins_agree(self):
        rows = [
            [p.strip() for p in line.split("|", 3)]
            for line in (FIXTURES / "watch-cases.txt").read_text().splitlines()
            if line.strip() and not line.startswith("#")
        ]
        names = [r[0] for r in rows]
        self.assertEqual(len(names), len(set(names)))
        pins = [
            line.split()[0]
            for line in (FIXTURES / "watch-expected.txt").read_text().splitlines()
        ]
        self.assertEqual(sorted(names), sorted(pins))
        for name, project, args, steps in rows:
            self.assertTrue((FIXTURES / "projects" / project).is_dir(), name)
            self.assertIn("-w", shlex.split(args), name)
            for step in steps.split(" ;; "):
                self.assertTrue(
                    step.startswith(
                        (
                            "@wait",
                            "@idle",
                            "@exits",
                            "@replace",
                            "@write",
                            "@append",
                            "@remove",
                            "@mkdir",
                            "@rename",
                            "@touch",
                        )
                    ),
                    f"{name}: {step}",
                )


if __name__ == "__main__":
    unittest.main()
