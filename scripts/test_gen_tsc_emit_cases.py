import re
import subprocess
import sys
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
GENERATOR = SCRIPTS_DIR / "gen-tsc-emit-cases.py"
FIXTURES = SCRIPTS_DIR / "tsc-fixtures"
PROJECT = FIXTURES / "projects" / "emit"
INPUT = re.compile(r"^[\w./-]+\.(?:ts|tsx|mts|cts|js|json)$")


def emit_cases():
    for path in sorted(FIXTURES.glob("cases-emit-*.txt")):
        for line in path.read_text().splitlines():
            if line.strip() and not line.startswith("#"):
                yield path, [p.strip() for p in line.split("|", 3)]


class EmitMatrixTest(unittest.TestCase):
    def test_generated_files_are_current(self):
        r = subprocess.run(
            [sys.executable, "-I", str(GENERATOR), "--check"],
            capture_output=True,
            text=True,
        )
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)

    def test_matrix_is_large_and_well_formed(self):
        names = set()
        for path, (name, project, codes, args) in emit_cases():
            self.assertNotIn(name, names)
            names.add(name)
            self.assertEqual(project, "emit", name)
            self.assertEqual(len(codes.split()), len(args.split(" ;; ")), name)
            for tok in args.split():
                if INPUT.match(tok) and not tok.startswith("out/"):
                    self.assertTrue((PROJECT / tok).is_file(), f"{name}: {tok}")
        self.assertGreaterEqual(len(names), 100)

    def test_every_case_is_pinned(self):
        for path in sorted(FIXTURES.glob("cases-emit-*.txt")):
            pin = FIXTURES / path.name.replace("cases-", "expected-")
            self.assertTrue(pin.is_file(), f"missing {pin.name}")
            names = {c[0] for p, c in emit_cases() if p == path}
            self.assertEqual(
                {r.split()[0] for r in pin.read_text().splitlines() if r.strip()},
                names,
            )


if __name__ == "__main__":
    unittest.main()
