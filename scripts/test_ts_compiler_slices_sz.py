import importlib.util
import sys
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))
import ts_compiler_slices as slices  # noqa: E402
import ts_compiler_slices_sz as sz  # noqa: E402
from test_run_ts_compiler_oracle import entries  # noqa: E402

wrapper_spec = importlib.util.spec_from_file_location(
    "run_ts_compiler_sz_oracle", SCRIPTS_DIR / "run-ts-compiler-sz-oracle.py"
)
wrapper = importlib.util.module_from_spec(wrapper_spec)
wrapper_spec.loader.exec_module(wrapper)

SUITES = slices.suites_from_titles(
    entries(
        "Rr.ts",
        "scope.ts",
        "Tt.ts",
        "unused.ts",
        "v.ts",
        "while.ts",
        "yield.ts",
        "Aa.ts",
        "ss.ts|target=es5",
        "ss.ts|target=es2015",
    )
)


class SzSlicesTest(unittest.TestCase):
    def names(self, spec):
        return [s.base for s in slices.select(spec, SUITES, sz.SLICES)]

    def test_each_letter_row_selects_its_letter_case_insensitively(self):
        self.assertEqual(self.names("s"), ["scope.ts", "ss.ts", "ss.ts"])
        self.assertEqual(self.names("t"), ["Tt.ts"])
        self.assertEqual(self.names("y"), ["yield.ts"])

    def test_letter_rows_partition_the_range(self):
        union = [b for letter in sz.LETTERS for b in self.names(letter)]
        self.assertEqual(sorted(union), sorted(self.names("s-z")))
        self.assertNotIn("Rr.ts", union)
        self.assertNotIn("Aa.ts", union)

    def test_every_letter_is_a_slice_row(self):
        self.assertEqual(set(sz.LETTERS) | {"s-z"}, set(sz.SLICES))

    def test_known_failures_file_is_a_comment_header_plus_tab_separated_entries(self):
        known = Path(wrapper.KNOWN)
        self.assertTrue(known.exists())
        for line in known.read_text(encoding="utf-8").splitlines():
            if line and not line.startswith("#"):
                self.assertEqual(len(line.split("\t")), 2, line)

    def test_wrapper_rejects_an_unknown_letter(self):
        self.assertEqual(wrapper.main(["--letters", "s,z"]), 2)


if __name__ == "__main__":
    unittest.main()
