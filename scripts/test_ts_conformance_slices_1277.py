import importlib.util
import sys
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))
import ts_compiler_slices as compiler_slices  # noqa: E402
import ts_conformance_slices as conf  # noqa: E402
import ts_conformance_slices_1277 as es6  # noqa: E402

spec = importlib.util.spec_from_file_location(
    "run_ts_conformance_es6_oracle", SCRIPTS_DIR / "run-ts-conformance-es6-oracle.py"
)
wrapper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wrapper)

DIRS = [
    "es6/Symbols",
    "es6/arrowFunction",
    "es6/computedProperties",
    "es6/destructuring",
    "es6/modules",
    "es6/spread",
    "es6/yieldExpressions",
    "es7/exponentiationOperator",
    "es2016/x",
    "esnext/y",
    "classes/z",
]


def suites():
    out = []
    for i, d in enumerate(DIRS):
        full = f"tests/cases/conformance/{d}/f{i}.ts"
        key = f"conformance tests conformance tests for {full}"
        out += [
            (f"{key} {it} for {full}", f"{it} for {full}")
            for it in compiler_slices.IT_TITLES
        ]
    return conf.suites_from_titles(out)


class Es6Slices(unittest.TestCase):
    def test_rows_are_dir_terms_below_es6(self):
        for name, spec in es6.CHUNKS.items():
            self.assertTrue(spec.startswith("dir:es6/"), name)

    def test_chunks_partition_the_listed_subdirectories(self):
        everything = suites()
        whole = {s.key for s in conf.select("es6", everything, es6.SLICES)}
        self.assertEqual(len(whole), 7)
        seen = {}
        for name in es6.CHUNKS:
            for s in conf.select(name, everything, es6.SLICES):
                self.assertNotIn(s.key, seen)
                seen[s.key] = name
        self.assertEqual(set(seen), whole)

    def test_no_directory_in_two_chunks(self):
        dirs = [d for ds in es6.CHUNK_DIRS.values() for d in ds]
        self.assertEqual(len(dirs), len(set(dirs)))
        self.assertEqual(len(dirs), 28)

    def test_es6_row_is_pinned_and_chunks_are_not(self):
        self.assertEqual(conf.top_dirs("es6", es6.SLICES), ["es6"])
        self.assertIsNone(conf.top_dirs("es1", es6.SLICES))

    def test_known_failures_file_is_the_wrappers(self):
        self.assertEqual(Path(wrapper.KNOWN).name, "conformance-es6-known-failures.txt")
        self.assertTrue(Path(wrapper.KNOWN).exists())


if __name__ == "__main__":
    unittest.main()
