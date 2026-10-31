import importlib.util
import os
import sys
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
ROOT = SCRIPTS_DIR.parent
sys.path.insert(0, str(SCRIPTS_DIR))
import ts_compiler_slices as slices  # noqa: E402
import ts_compiler_slices_1272 as mine  # noqa: E402

ITS = slices.IT_TITLES
TABLE = {**slices.SLICES, **mine.SLICES}


def load_oracle():
    spec = importlib.util.spec_from_file_location(
        "oracle", SCRIPTS_DIR / "run-ts-compiler-oracle.py"
    )
    oracle = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(oracle)
    return oracle


def suites(*files):
    entries = []
    for f in files:
        key = f"compiler tests compiler tests for tests/cases/compiler/{f}"
        entries += [(f"{key} {it} for {f}", f"{it} for {f}") for it in ITS]
    return slices.suites_from_titles(entries)


class Slice1272Test(unittest.TestCase):
    def test_row_selects_m_to_r_case_insensitively(self):
        all_suites = suites(
            "2dArrays.ts",
            "lastOne.ts",
            "mapped.ts",
            "Nn.tsx",
            "q.ts",
            "ReturnX.ts",
            "s.ts",
        )
        picked = [s.base for s in slices.select("issue1272", all_suites, TABLE)]
        self.assertEqual(picked, ["mapped.ts", "Nn.tsx", "q.ts", "ReturnX.ts"])

    def test_row_is_pinned_to_the_oracle_json_count(self):
        oracle = load_oracle()
        self.assertEqual(oracle.pinned_expected("issue1272", TABLE), 9048)

    def test_known_failures_file_is_wellformed(self):
        oracle = load_oracle()
        known = oracle.load_known(
            str(ROOT / "ts-suite" / "compiler-known-failures-m-r.txt")
        )
        for title, issue in known.items():
            self.assertTrue(title.startswith("compiler tests "), title)
            self.assertRegex(issue, r"^#\d+$")

    def test_wrapper_is_executable(self):
        wrapper = SCRIPTS_DIR / "run-ts-compiler-m-r-oracle.sh"
        self.assertTrue(os.access(wrapper, os.X_OK))
        self.assertIn("ts_compiler_slices_1272", wrapper.read_text())


if __name__ == "__main__":
    unittest.main()
