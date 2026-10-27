import os
import re
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
ORACLE = SCRIPTS_DIR / "run-tsc-projects-oracle.py"
FIXTURES = SCRIPTS_DIR / "tsc-fixtures"

# Prints what an engine would observe: the environment variable, whether stdout is a
# terminal and its size, and the bytes of a.ts.
PROBE = """#!/bin/sh
if [ -t 1 ]; then size=$(stty size); else size=pipe; fi
echo "foo=$FOO size=$size"
od -An -c a.ts b.ts
"""

NODE_EXPECTED = {
    "plain": "foo= size=pipe",
    "env": "foo=bar size=pipe",
    "pty": "foo= size=30 90",
}


def write_exe(path, text):
    path.write_text(text)
    path.chmod(path.stat().st_mode | stat.S_IEXEC)


class CasesFilesTest(unittest.TestCase):
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
        (self.fix / "projects" / "p" / "a.ts.esc").write_text("a\\r\\n\\u2028\\xe9\\n")
        (self.fix / "projects" / "p" / "b.ts.utf16le.esc").write_text("\\ufeffA\n")
        (self.fix / "cases.txt").write_text("base | p | 0 | x\n")
        (self.fix / "cases-more.txt").write_text(
            "plain | p | 0 | x\nenv | p | 0 | FOO=bar x\npty | p | 0 | @pty:30x90 x\n"
        )
        self.bin = root / "bin"
        self.bin.mkdir()
        write_exe(self.bin / "jsse", PROBE)
        write_exe(self.bin / "node", self.node_script())

    def node_script(self, overrides=None):
        lines = ["#!/bin/sh", 'case "$(pwd)" in']
        expected = {**NODE_EXPECTED, **(overrides or {})}
        for name, text in expected.items():
            lines.append(f'  */{name}) echo "{text}" ;;')
        lines.append('  *) echo "foo= size=pipe" ;;')
        lines.append("esac")
        lines.append(
            "printf '   a  \\\\r  \\\\n 342 200 250 303 251  \\\\n 377 376   A  \\\\0\\n'"
        )
        return "\n".join(lines) + "\n"

    def run_oracle(self, *extra):
        env = {k: v for k, v in os.environ.items() if k != "TSC_DIR"}
        env["PATH"] = f"{self.bin}:{env['PATH']}"
        env["JSSE"] = str(self.bin / "jsse")
        env["TSC_DIR"] = str(self.tsc)
        return subprocess.run(
            [sys.executable, "-I", str(ORACLE), "--fixtures", str(self.fix), *extra],
            capture_output=True,
            text=True,
            env=env,
            check=False,
        )

    def test_extra_cases_files_run_and_get_their_own_pin_file(self):
        r = self.run_oracle("--pin", "--jobs", "2")
        self.assertIn("passed", r.stdout)
        self.assertTrue((self.fix / "expected-more.txt").is_file(), r.stdout)
        more = (self.fix / "expected-more.txt").read_text().split()
        base = (self.fix / "expected-projects.txt").read_text().split()
        self.assertEqual(more[0::2], ["plain", "env", "pty"])
        self.assertEqual(base[0::2], ["base"])

    def test_pins_of_every_file_are_checked_without_node(self):
        self.run_oracle("--pin", "--jobs", "2")
        (self.bin / "node").unlink()
        r = self.run_oracle("--no-node", "--jobs", "2")
        self.assertIn("4/4 passed", r.stdout, r.stdout)
        (self.fix / "expected-more.txt").write_text("plain 00\nenv 00\npty 00\n")
        r = self.run_oracle("--no-node", "--jobs", "2")
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("1/4 passed", r.stdout)

    def test_environment_and_terminal_directives_reach_the_engine(self):
        write_exe(self.bin / "node", self.node_script({"env": "foo=nope size=pipe"}))
        r = self.run_oracle("--jobs", "2", "env")
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("FAIL (stdout)", r.stdout)
        write_exe(self.bin / "node", self.node_script({"pty": "foo= size=24 80"}))
        r = self.run_oracle("--jobs", "2", "pty")
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("FAIL (stdout)", r.stdout)

    def test_escaped_sources_are_decoded_before_the_engines_see_them(self):
        r = self.run_oracle("--jobs", "2", "plain", "env", "pty")
        self.assertEqual(r.returncode, 0, r.stdout)
        self.assertIn("3/3 passed", r.stdout)


class DiagnosticsFixtureTest(unittest.TestCase):
    def cases(self):
        rows = []
        for line in (FIXTURES / "cases-diagnostics.txt").read_text().splitlines():
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
        pinned = (FIXTURES / "expected-diagnostics.txt").read_text().split()
        self.assertEqual(set(pinned[0::2]), names)

    def test_thirty_error_fixtures_are_compiled_plain_and_pretty(self):
        fixtures = [c for c in self.cases() if re.match(r"diag-e\d\d-", c[0])]
        self.assertGreaterEqual(len(fixtures), 30)
        for name, _, _, args in fixtures:
            plain, pretty = args.split(" ;; ")
            self.assertIn("--pretty false", plain, name)
            self.assertIn("--pretty ", pretty, name)

    def test_every_locale_directory_of_the_package_is_exercised(self):
        text = (FIXTURES / "cases-diagnostics.txt").read_text()
        for locale in "cs de es fr it ja ko pl pt-br ru tr zh-cn zh-tw".split():
            self.assertIn(f"diag-locale-{locale} |", text)
            self.assertIn(f"--locale {locale} --help", text)


if __name__ == "__main__":
    unittest.main()
