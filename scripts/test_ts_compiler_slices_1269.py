import importlib.util
import os
import sys
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
ROOT = SCRIPTS_DIR.parent
sys.path.insert(0, str(SCRIPTS_DIR))
import ts_compiler_slices as slices  # noqa: E402
import ts_compiler_slices_1269 as mine  # noqa: E402

ITS = slices.IT_TITLES


def suites(*files):
    entries = []
    for f in files:
        key = f"compiler tests compiler tests for tests/cases/compiler/{f}"
        entries += [(f"{key} {it} for {f}", f"{it} for {f}") for it in ITS]
    return slices.suites_from_titles(entries)


class Slice1269Test(unittest.TestCase):
    def test_row_selects_digits_and_a_to_c_case_insensitively(self):
        all_suites = suites(
            "2dArrays.ts", "APILibCheck.ts", "b.ts", "Cc.tsx", "d.ts", "E.ts"
        )
        table = dict(slices.SLICES)
        table.update(mine.SLICES)
        picked = [s.base for s in slices.select("issue1269", all_suites, table)]
        self.assertEqual(picked, ["2dArrays.ts", "APILibCheck.ts", "b.ts", "Cc.tsx"])
        self.assertEqual(picked, [s.base for s in slices.select("a-c", all_suites)])

    def test_known_failures_file_is_wellformed(self):
        spec = importlib.util.spec_from_file_location(
            "oracle", SCRIPTS_DIR / "run-ts-compiler-oracle.py"
        )
        oracle = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(oracle)
        known = oracle.load_known(
            str(ROOT / "ts-suite" / "compiler-known-failures-0-9-a-c.txt")
        )
        for title, issue in known.items():
            self.assertTrue(title.startswith("compiler tests "), title)
            self.assertRegex(issue, r"^#\d+$")

    def test_wrapper_is_executable(self):
        wrapper = SCRIPTS_DIR / "run-ts-compiler-0-9-a-c-oracle.sh"
        self.assertTrue(os.access(wrapper, os.X_OK))
        self.assertIn("ts_compiler_slices_1269", wrapper.read_text())


if __name__ == "__main__":
    unittest.main()
