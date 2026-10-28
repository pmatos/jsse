import os
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
ORACLE = SCRIPTS_DIR / "run-tsc-projects-oracle.py"
FIXTURES = SCRIPTS_DIR / "tsc-fixtures"

NODE = """#!/bin/sh
echo "out $2 $(pwd) 4:10:21 AM"
echo "err" >&2
echo "same" > emitted.txt
echo "$(pwd)" > path.txt
exit 2
"""


def write_exe(path, text):
    path.write_text(text)
    path.chmod(path.stat().st_mode | stat.S_IEXEC)


class OracleHarnessTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        root = Path(self.tmp.name)
        self.tsc = root / "typescript"
        (self.tsc / "lib").mkdir(parents=True)
        (self.tsc / "lib" / "_tsc.js").write_text("")
        (self.tsc / "package.json").write_text(
            '{"name": "typescript", "version": "6.0.3"}'
        )
        self.fix = root / "fix"
        (self.fix / "projects" / "p").mkdir(parents=True)
        (self.fix / "projects" / "p" / "a.ts").write_text("export {}")
        (self.fix / "cases.txt").write_text(
            "# comment\none | p | 2 | --noEmit a.ts\ntwo | p | 2 2 | a.ts ;; b.ts\n"
        )
        self.bin = root / "bin"
        self.bin.mkdir()
        write_exe(self.bin / "node", NODE)

    def run_oracle(self, jsse_body, *extra, tsc_dir=True):
        jsse = Path(self.tmp.name) / "jsse"
        write_exe(jsse, jsse_body)
        env = {k: v for k, v in os.environ.items() if k != "TSC_DIR"}
        env["PATH"] = f"{self.bin}:{env['PATH']}"
        env["JSSE"] = str(jsse)
        if tsc_dir:
            env["TSC_DIR"] = str(self.tsc)
        return subprocess.run(
            [
                sys.executable,
                "-I",
                str(ORACLE),
                "--fixtures",
                str(self.fix),
                "--jobs",
                "2",
                *extra,
            ],
            capture_output=True,
            text=True,
            env=env,
        )

    def test_identical_engines_pass_despite_paths_and_clock_stamps(self):
        jsse = NODE.replace("4:10:21 AM", "9:59:01 PM").replace("$2", "$3")
        r = self.run_oracle(jsse)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("tsc project oracle: 2/2 passed", r.stdout)

    def test_a_case_without_arguments_runs(self):
        (self.fix / "cases.txt").write_text("bare | p | 2 |\n")
        r = self.run_oracle(NODE.replace("$2", "$3"))
        self.assertNotIn("Traceback", r.stderr)
        self.assertIn("bare", r.stdout)

    def test_stdout_difference_fails(self):
        r = self.run_oracle(NODE.replace("out $2", "OUT $3"))
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("FAIL (stdout)", r.stdout)

    def test_emitted_file_difference_fails(self):
        r = self.run_oracle(NODE.replace("same", "different"))
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("file emitted.txt", r.stdout)

    def test_exit_code_difference_fails(self):
        r = self.run_oracle(NODE.replace("exit 2", "exit 1").replace("$2", "$3"))
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("exit", r.stdout)

    def test_fixture_expecting_another_exit_code_fails_on_both_engines(self):
        (self.fix / "cases.txt").write_text("one | p | 0 | --noEmit a.ts\n")
        r = self.run_oracle(NODE.replace("$2", "$3"))
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("node exit status, jsse exit status", r.stdout)

    def test_pinned_digests_replace_node(self):
        jsse = NODE.replace("$2", "$3")
        self.assertEqual(self.run_oracle(jsse, "--pin").returncode, 0)
        self.assertTrue((self.fix / "expected-projects.txt").is_file())
        (self.bin / "node").unlink()
        r = self.run_oracle(jsse, "--no-node")
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        r = self.run_oracle(jsse.replace("same", "drift"), "--no-node")
        self.assertEqual(r.returncode, 1, r.stdout)

    def test_pin_merges_single_cases_and_skips_wrong_exit_codes(self):
        jsse = NODE.replace("$2", "$3")
        self.assertEqual(self.run_oracle(jsse, "--pin").returncode, 0)
        pins = self.fix / "expected-projects.txt"
        full = pins.read_text()
        self.assertEqual(self.run_oracle(jsse, "--pin", "one").returncode, 0)
        self.assertEqual(pins.read_text(), full)
        (self.fix / "cases.txt").write_text(
            "one | p | 0 | --noEmit a.ts\ntwo | p | 2 2 | a.ts ;; b.ts\n"
        )
        self.assertEqual(self.run_oracle(jsse, "--pin").returncode, 1)
        self.assertEqual(
            [row.split()[0] for row in pins.read_text().splitlines()], ["one", "two"]
        )
        self.assertEqual(pins.read_text(), full)

    def test_pin_refuses_no_node(self):
        r = self.run_oracle(NODE, "--pin", "--no-node")
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("--pin needs Node", r.stdout + r.stderr)

    def test_empty_directory_difference_fails(self):
        r = self.run_oracle(
            NODE.replace("exit 2", "mkdir -p only-here\nexit 2").replace("$2", "$3"),
            "one",
        )
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("file only-here", r.stdout)

    def test_skips_without_tsc_dir_and_fails_when_required(self):
        r = self.run_oracle(NODE, tsc_dir=False)
        self.assertEqual(r.returncode, 0)
        self.assertIn("SKIP: TSC_DIR is not set", r.stdout)
        r = self.run_oracle(NODE, "--require", tsc_dir=False)
        self.assertEqual(r.returncode, 3)

    def test_skips_other_typescript_versions(self):
        (self.tsc / "package.json").write_text('{"version": "5.9.3"}')
        r = self.run_oracle(NODE)
        self.assertEqual(r.returncode, 0)
        self.assertIn("found 5.9.3", r.stdout)


class FixtureTest(unittest.TestCase):
    def test_cases_are_well_formed(self):
        names = set()
        for line in (FIXTURES / "cases.txt").read_text().splitlines():
            if not line.strip() or line.startswith("#"):
                continue
            name, project, codes, args = (p.strip() for p in line.split("|", 3))
            self.assertNotIn(name, names)
            names.add(name)
            self.assertTrue((FIXTURES / "projects" / project).is_dir(), name)
            self.assertEqual(len(codes.split()), len(args.split(" ;; ")), name)
        pinned = (FIXTURES / "expected-projects.txt").read_text().split()
        self.assertEqual(set(pinned[0::2]), names)


if __name__ == "__main__":
    unittest.main()
