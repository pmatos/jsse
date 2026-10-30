import importlib.util
import sys
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))
import ts_compiler_slices as compiler_slices  # noqa: E402
import ts_conformance_slices_1282 as conf  # noqa: E402

spec = importlib.util.spec_from_file_location(
    "run_ts_conformance_1282_oracle", SCRIPTS_DIR / "run-ts-conformance-1282-oracle.py"
)
wrapper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wrapper)


def entries(*files):
    out = []
    for f in files:
        path, _, cfg = f.partition("|")
        full = f"tests/cases/conformance/{path}"
        suite = f"conformance tests conformance tests for {full}"
        if cfg:
            suite += f" ({cfg})"
        for it in compiler_slices.IT_TITLES:
            out.append((f"{suite} {it} for {full}", f"{it} for {full}"))
    return out


SUITES = conf.suites_from_titles(
    entries(
        "jsx/a.tsx",
        "jsx/a.tsx|@target: es5",
        "jsdoc/b/c.ts",
        "salsa/d.js",
        "typings/e.ts",
        "types/f.ts",
        "jsxLike/g.ts",
        "classes/h.ts",
    )
)


class Conformance1282SlicesTest(unittest.TestCase):
    def files(self, spec):
        return [s.rel for s in conf.select(spec, SUITES)]

    def test_rows_name_the_four_directories_and_their_union(self):
        self.assertEqual(self.files("jsx"), ["jsx/a.tsx", "jsx/a.tsx"])
        self.assertEqual(self.files("jsdoc"), ["jsdoc/b/c.ts"])
        self.assertEqual(self.files("salsa"), ["salsa/d.js"])
        self.assertEqual(self.files("typings"), ["typings/e.ts"])
        union = [s for d in conf.GROUP_1282 for s in conf.select(d, SUITES)]
        whole = conf.select("issue1282", SUITES)
        self.assertEqual(sorted(s.key for s in union), sorted(s.key for s in whole))
        self.assertEqual({s.dir for s in whole}, {"jsx", "jsdoc", "salsa", "typings"})

    def test_dir_term_and_compiler_terms_still_work(self):
        self.assertEqual(self.files("dir:jsdoc/b"), ["jsdoc/b/c.ts"])
        self.assertEqual(self.files("files:g.ts"), ["jsxLike/g.ts"])
        self.assertEqual(self.files("dir:jsx+files:a.tsx"), ["jsx/a.tsx"] * 2)

    def test_pin_of_oracle_json_counts_the_group(self):
        pinned = wrapper.wrapper.pinned_expected
        self.assertEqual(pinned("jsx", conf.SLICES), 1392)
        self.assertEqual(pinned("jsdoc", conf.SLICES), 2382)
        self.assertEqual(pinned("salsa", conf.SLICES), 1158)
        self.assertEqual(pinned("typings", conf.SLICES), 54)
        self.assertEqual(pinned("issue1282", conf.SLICES), 4986)
        self.assertIsNone(pinned("dir:jsdoc/b", conf.SLICES))

    def test_known_failures_file_is_its_own_and_well_formed(self):
        known = Path(wrapper.KNOWN)
        self.assertEqual(known.name, "conformance-1282-known-failures.txt")
        self.assertTrue(known.exists())
        for line in known.read_text(encoding="utf-8").splitlines():
            if line and not line.startswith("#"):
                self.assertEqual(len(line.split("\t")), 2, line)


if __name__ == "__main__":
    unittest.main()
