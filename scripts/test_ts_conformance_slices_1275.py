import sys
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))
import ts_conformance_slices_1275 as conf  # noqa: E402

ITS = [
    "Correct errors",
    "Correct module resolution tracing",
    "Correct sourcemap content",
    "Correct JS output",
    "Correct Sourcemap output",
    "Correct type/symbol baselines",
]


FILES = [
    "es6/a.ts",
    "expressions/arrayLiterals/x.ts",
    "expressions/binaryOperators/y.ts|@target: es5",
    "expressions/binaryOperators/y.ts|@target: es2015",
    "expressions/functionCalls/z.ts",
    "expressions/nullishCoalescingOperator/n.ts",
    "expressions/objectLiterals/o.ts",
    "expressions/optionalChaining/p.ts",
    "expressions/typeGuards/g.ts",
    "expressions/valuesAndReferences/v.ts",
    "expressionsLike/w.ts",
    "types/t.ts",
]


def suites():
    out = []
    for f in FILES:
        path, _, cfg = f.partition("|")
        key = f"conformance tests conformance tests for tests/cases/conformance/{path}"
        if cfg:
            key += f" ({cfg})"
        out += [(f"{key} {it} for {path}", f"{it} for {path}") for it in ITS]
    return conf.suites_from_titles(out)


class Slices(unittest.TestCase):
    def test_suite_parsing(self):
        by_key = {s.key: s for s in suites()}
        s = by_key[
            "conformance tests conformance tests for tests/cases/conformance/expressions/binaryOperators/y.ts (@target: es5)"
        ]
        self.assertEqual(
            s.file, "tests/cases/conformance/expressions/binaryOperators/y.ts"
        )
        self.assertEqual(s.config, "@target: es5")
        self.assertEqual(s.base, "y.ts")
        self.assertEqual(len(s.tests), 6)

    def test_compiler_titles_are_not_conformance_suites(self):
        title = "compiler tests compiler tests for tests/cases/compiler/a.ts"
        self.assertEqual(
            conf.suites_from_titles(
                [(f"{title} Correct errors for a.ts", "Correct errors for a.ts")]
            ),
            [],
        )

    def test_dir_selects_whole_directories_only(self):
        got = [s.file for s in conf.select("expressions", suites())]
        self.assertEqual(len(got), 9)
        self.assertTrue(all("/conformance/expressions/" in f for f in got))
        self.assertEqual(
            conf.select("dir:expressions/typeGuards", suites())[0].base, "g.ts"
        )

    def test_chunks_partition_expressions(self):
        every = suites()
        whole = [s.key for s in conf.select("expressions", every)]
        seen = []
        for name in conf.CHUNKS:
            seen += [s.key for s in conf.select(name, every)]
        self.assertEqual(sorted(seen), sorted(whole))

    def test_chunk_directories_are_the_group(self):
        named = [
            d.split("/", 1)[1]
            for spec in conf.CHUNKS.values()
            for d in conf.dirs_of(spec[4:])
        ]
        self.assertEqual(sorted(named), sorted(conf.GROUP))

    def test_other_terms_delegate(self):
        every = suites()
        self.assertEqual(len(conf.select("first:2", every)), 2)
        self.assertEqual([s.base for s in conf.select("files:w.ts", every)], ["w.ts"])
        both = conf.select(
            "dir:expressions/valuesAndReferences+match:typeGuards", every
        )
        self.assertEqual([s.base for s in both], ["g.ts", "v.ts"])

    def test_prefix_is_rejected(self):
        with self.assertRaises(ValueError):
            conf.select("prefix:A-C", suites())

    def test_grep_selects_the_six_tests_of_a_suite(self):
        every = suites()
        grep = conf.grep_for(conf.select("dir:expressions/typeGuards", every))
        self.assertTrue(grep.endswith(") Correct "))
        self.assertIn("conformance\\/expressions\\/typeGuards\\/g\\.ts", grep)

    def test_pinned_counts_come_from_the_oracle_json(self):
        self.assertEqual(conf.pinned_expected("expressions", conf.SLICES), 2574)
        self.assertIsNone(conf.pinned_expected("ex1", conf.SLICES))
        self.assertIsNone(conf.pinned_expected("first:3", conf.SLICES))


if __name__ == "__main__":
    unittest.main()
