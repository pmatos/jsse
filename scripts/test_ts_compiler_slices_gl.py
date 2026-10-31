import importlib.util
import sys
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))
import ts_compiler_slices as slices  # noqa: E402
import ts_compiler_slices_gl as gl  # noqa: E402
from test_run_ts_compiler_oracle import entries  # noqa: E402

wrapper_spec = importlib.util.spec_from_file_location(
    "run_ts_compiler_gl_oracle", SCRIPTS_DIR / "run-ts-compiler-gl-oracle.py"
)
wrapper = importlib.util.module_from_spec(wrapper_spec)
wrapper_spec.loader.exec_module(wrapper)

SUITES = slices.suites_from_titles(
    entries(
        "Fa.ts",
        "genericCall.ts",
        "H.ts",
        "ifElse.ts",
        "jsx.tsx",
        "Kk.ts",
        "lib.ts",
        "m.ts",
        "gg.ts|target=es5",
        "gg.ts|target=es2015",
    )
)


class GlSlicesTest(unittest.TestCase):
    def names(self, spec):
        return [s.base for s in slices.select(spec, SUITES, gl.SLICES)]

    def test_each_letter_row_selects_its_letter_case_insensitively(self):
        self.assertEqual(self.names("g"), ["genericCall.ts", "gg.ts", "gg.ts"])
        self.assertEqual(self.names("h"), ["H.ts"])
        self.assertEqual(self.names("k"), ["Kk.ts"])

    def test_letter_rows_partition_the_range(self):
        union = [b for letter in gl.LETTERS for b in self.names(letter)]
        self.assertEqual(sorted(union), sorted(self.names("g-l")))
        self.assertNotIn("Fa.ts", union)
        self.assertNotIn("m.ts", union)

    def test_every_letter_is_a_slice_row(self):
        self.assertEqual(set(gl.LETTERS) | {"g-l"}, set(gl.SLICES))

    def test_known_failures_file_is_a_comment_header_plus_tab_separated_entries(self):
        known = Path(wrapper.KNOWN)
        self.assertTrue(known.exists())
        for line in known.read_text(encoding="utf-8").splitlines():
            if line and not line.startswith("#"):
                self.assertEqual(len(line.split("\t")), 2, line)

    def test_wrapper_rejects_an_unknown_letter(self):
        self.assertEqual(wrapper.main(["--letters", "g,z"]), 2)


if __name__ == "__main__":
    unittest.main()
