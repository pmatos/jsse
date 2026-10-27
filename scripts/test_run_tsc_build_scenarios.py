import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_run_tsc_projects_oracle import ORACLE, write_exe

FIXTURES = Path(__file__).resolve().parent / "tsc-fixtures"

ENGINE = """#!/bin/sh
cat a.ts
echo "mtime $(stat -c %Y a.ts)"
[ -e gone.ts ] && echo present || echo absent
exit 0
"""


class ScenarioStepTest(unittest.TestCase):
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
        write_exe(self.bin / "node", ENGINE)
        write_exe(root / "jsse", ENGINE)

    def run_oracle(self, *extra):
        env = {k: v for k, v in os.environ.items() if k != "TSC_DIR"}
        env["PATH"] = f"{self.bin}:{env['PATH']}"
        env["JSSE"] = str(Path(self.tmp.name) / "jsse")
        env["TSC_DIR"] = str(self.tsc)
        return subprocess.run(
            [sys.executable, "-I", str(ORACLE), "--fixtures", str(self.fix), *extra],
            capture_output=True,
            text=True,
            env=env,
        )

    def test_steps_edit_touch_and_remove_before_the_next_invocation(self):
        (self.fix / "cases.txt").write_text(
            "steps | p | 0 0 | @utimes 1600000000 . ;; a.ts ;; "
            "@replace a.ts one two ;; @utimes 1800000000 a.ts ;; @remove gone.ts ;; "
            "@mtimes-after 1700000000 a.ts ;; a.ts\n"
        )
        r = self.run_oracle("--ci")
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("tsc project oracle: 1/1 passed", r.stdout)

    def test_a_step_that_changes_one_engine_only_is_seen(self):
        (self.fix / "cases.txt").write_text(
            "steps | p | 0 0 | a.ts ;; @replace a.ts one two ;; a.ts\n"
        )
        write_exe(
            self.bin / "node",
            ENGINE.replace("cat a.ts", "echo different"),
        )
        r = self.run_oracle("--ci")
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("stdout", r.stdout)

    def test_unknown_and_unmatched_steps_abort(self):
        (self.fix / "cases.txt").write_text("bad | p | 0 | @nope x ;; a.ts\n")
        self.assertNotEqual(self.run_oracle().returncode, 0)
        (self.fix / "cases.txt").write_text(
            "bad | p | 0 | @replace a.ts absent x ;; a.ts\n"
        )
        r = self.run_oracle()
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("not found in a.ts", r.stdout + r.stderr)

    def test_topic_case_files_run_with_their_own_pins(self):
        (self.fix / "cases.txt").write_text("base | p | 0 | a.ts\n")
        (self.fix / "cases-extra.txt").write_text("more | p | 0 0 | a.ts ;; a.ts\n")
        r = self.run_oracle("--ci", "--pin")
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertEqual(
            [
                row.split()[0]
                for row in (self.fix / "expected-projects.txt").read_text().splitlines()
            ],
            ["base"],
        )
        self.assertEqual(
            [
                row.split()[0]
                for row in (self.fix / "expected-extra.txt").read_text().splitlines()
            ],
            ["more"],
        )
        (self.bin / "node").unlink()
        r = self.run_oracle("--ci", "--no-node")
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("2/2 passed", r.stdout)


class BuildFixtureTest(unittest.TestCase):
    def test_build_cases_are_well_formed_and_pinned(self):
        names = set()
        steps_seen = set()
        for line in (FIXTURES / "cases-build.txt").read_text().splitlines():
            if not line.strip() or line.startswith("#"):
                continue
            name, project, codes, steps = (p.strip() for p in line.split("|", 3))
            self.assertNotIn(name, names)
            names.add(name)
            self.assertTrue((FIXTURES / "projects" / project).is_dir(), name)
            commands = [s for s in steps.split(" ;; ") if not s.startswith("@")]
            steps_seen |= {
                s.split()[0] for s in steps.split(" ;; ") if s.startswith("@")
            }
            self.assertEqual(len(codes.split()), len(commands), name)
        self.assertEqual(
            steps_seen, {"@utimes", "@replace", "@remove", "@mtimes-after"}
        )
        pinned = (FIXTURES / "expected-build.txt").read_text().split()
        self.assertEqual(set(pinned[0::2]), names)
        base = {
            line.split("|")[0].strip()
            for line in (FIXTURES / "cases.txt").read_text().splitlines()
            if line.strip() and not line.startswith("#")
        }
        self.assertFalse(base & names)


if __name__ == "__main__":
    unittest.main()
