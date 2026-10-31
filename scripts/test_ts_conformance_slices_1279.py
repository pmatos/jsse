import sys
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))
import ts_conformance_slices_1275 as conf  # noqa: E402
import ts_conformance_slices_1279 as group  # noqa: E402

ITS = [
    "Correct errors",
    "Correct module resolution tracing",
    "Correct sourcemap content",
    "Correct JS output",
    "Correct Sourcemap output",
    "Correct type/symbol baselines",
]

FILES = [
    "statements/VariableStatements/a.ts",
    "statements/VariableStatements/b.ts|@target: es5",
    "statements/for-ofStatements/c.ts",
    "statements/for-inStatements/d.ts",
    "statements/breakStatements/e.ts",
    "statements/tryStatements/f.ts",
    "controlFlow/g.ts",
    "functions/h.ts",
    "functions/i.ts|@strict: true",
    "types/t.ts",
    "statementsLike/w.ts",
    "expressions/typeGuards/x.ts",
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


def pick(spec):
    return conf.select(spec, suites(), {**conf.SLICES, **group.SLICES})


class Slices(unittest.TestCase):
    def test_issue_row_selects_the_three_directories_only(self):
        got = [s.base for s in pick("issue1279")]
        self.assertEqual(
            sorted(got),
            ["a.ts", "b.ts", "c.ts", "d.ts", "e.ts", "f.ts", "g.ts", "h.ts", "i.ts"],
        )

    def test_chunks_partition_the_group(self):
        seen = []
        for name in group.CHUNKS:
            seen += [s.key for s in pick(name)]
        self.assertEqual(sorted(seen), sorted(s.key for s in pick("issue1279")))

    def test_chunks_split_statements_by_directory(self):
        self.assertEqual(sorted(s.base for s in pick("st1")), ["a.ts", "b.ts"])
        self.assertEqual(sorted(s.base for s in pick("st2")), ["c.ts", "d.ts"])
        self.assertEqual(sorted(s.base for s in pick("st3")), ["e.ts", "f.ts"])
        self.assertEqual([s.base for s in pick("cf")], ["g.ts"])
        self.assertEqual(sorted(s.base for s in pick("fn")), ["h.ts", "i.ts"])

    def test_statements_directories_are_the_chunk_directories(self):
        named = [
            d.split("/", 1)[1]
            for name in ("st1", "st2", "st3")
            for d in conf.dirs_of(group.CHUNKS[name][4:])
        ]
        self.assertEqual(sorted(named), sorted(group.STATEMENTS))

    def test_pins_come_from_the_oracle_json(self):
        table = {**conf.SLICES, **group.SLICES}
        self.assertEqual(conf.pinned_expected("issue1279", table), 3648)
        self.assertEqual(conf.pinned_expected("statements", table), 3150)
        self.assertEqual(conf.pinned_expected("controlFlow", table), 348)
        self.assertEqual(conf.pinned_expected("functions", table), 150)
        self.assertIsNone(conf.pinned_expected("st1", table))


if __name__ == "__main__":
    unittest.main()
