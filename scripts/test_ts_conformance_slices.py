import importlib.util
import sys
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))
import ts_compiler_slices as compiler_slices  # noqa: E402
import ts_conformance_slices as conf  # noqa: E402

wrapper_spec = importlib.util.spec_from_file_location(
    "run_ts_conformance_oracle", SCRIPTS_DIR / "run-ts-conformance-oracle.py"
)
wrapper = importlib.util.module_from_spec(wrapper_spec)
wrapper_spec.loader.exec_module(wrapper)

ITS = compiler_slices.IT_TITLES


def entries(*files):
    out = []
    for f in files:
        path, _, cfg = f.partition("|")
        full = f"tests/cases/conformance/{path}"
        suite = f"conformance tests conformance tests for {full}"
        if cfg:
            suite += f" ({cfg})"
        for it in ITS:
            out.append((f"{suite} {it} for {full}", f"{it} for {full}"))
    return out


SUITES = conf.suites_from_titles(
    entries(
        "top.ts",
        "classes/a/Class1.ts",
        "classes/a/Class1.ts|@target: es5",
        "classes/b/Class2.tsx",
        "decorators/decorator.ts",
        "esDecorators/es.ts",
        "override/override1.ts",
        "classesX/other.ts",
        "types/t.ts",
        "classes/c/script.js",
        "classes/c/mod.mts",
    )
)


class ConformanceSlicesTest(unittest.TestCase):
    def files(self, spec):
        return [s.rel for s in conf.select(spec, SUITES)]

    def test_suite_titles_give_file_config_and_directory(self):
        s = SUITES[2]
        self.assertEqual(s.file, "tests/cases/conformance/classes/a/Class1.ts")
        self.assertEqual(s.config, "@target: es5")
        self.assertEqual((s.dir, s.base, len(s.tests)), ("classes", "Class1.ts", 6))
        self.assertEqual(SUITES[0].dir, "")

    def test_dir_term_selects_a_directory_not_a_name_prefix(self):
        self.assertEqual(
            self.files("dir:classes"),
            [
                "classes/a/Class1.ts",
                "classes/a/Class1.ts",
                "classes/b/Class2.tsx",
                "classes/c/script.js",
                "classes/c/mod.mts",
            ],
        )
        self.assertEqual(self.files("dir:classes/b"), ["classes/b/Class2.tsx"])
        self.assertEqual(
            self.files("dir:override,decorators"),
            [
                "decorators/decorator.ts",
                "override/override1.ts",
            ],
        )

    def test_compiler_terms_still_work_and_unions_do_not_duplicate(self):
        self.assertEqual(self.files("files:Class1.ts"), ["classes/a/Class1.ts"] * 2)
        self.assertEqual(
            self.files("dir:classes+files:Class1.ts+prefix:C"),
            [
                "classes/a/Class1.ts",
                "classes/a/Class1.ts",
                "classes/b/Class2.tsx",
                "classes/c/script.js",
                "classes/c/mod.mts",
            ],
        )

    def test_unknown_term_is_rejected(self):
        with self.assertRaises(ValueError):
            conf.select("nonsense:1", SUITES)

    def test_group_rows_name_the_four_directories(self):
        union = [s for d in conf.GROUP_1276 for s in conf.select(d, SUITES)]
        whole = conf.select("issue1276", SUITES)
        self.assertEqual(sorted(s.key for s in union), sorted(s.key for s in whole))
        self.assertNotIn("types", {s.dir for s in whole})
        self.assertNotIn("classesX", {s.dir for s in whole})

    def test_grep_selects_only_the_suites_of_the_batch(self):
        grep = conf.grep_for(SUITES[:1])
        self.assertTrue(grep.startswith("^(?:conformance tests conformance tests for "))
        self.assertTrue(grep.endswith(") Correct "))

    def test_pin_of_oracle_json_counts_the_directories(self):
        self.assertEqual(wrapper.pinned_expected("dir:override", conf.SLICES), 228)
        self.assertEqual(
            wrapper.pinned_expected("issue1276", conf.SLICES),
            228 + 840 + 1302 + 3924,
        )
        self.assertIsNone(wrapper.pinned_expected("dir:classes/b", conf.SLICES))
        self.assertIsNone(wrapper.pinned_expected("first:3", conf.SLICES))

    def test_known_failures_file_is_a_comment_header_plus_tab_separated_entries(self):
        known = Path(wrapper.KNOWN)
        self.assertTrue(known.exists())
        for line in known.read_text(encoding="utf-8").splitlines():
            if line and not line.startswith("#"):
                self.assertEqual(len(line.split("\t")), 2, line)


if __name__ == "__main__":
    unittest.main()
