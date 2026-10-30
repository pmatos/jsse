import sys
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))
import ts_conformance_slices_1275 as conf  # noqa: E402
import ts_conformance_slices_1283 as group  # noqa: E402

ITS = [
    "Correct errors",
    "Correct module resolution tracing",
    "Correct sourcemap content",
    "Correct JS output",
    "Correct Sourcemap output",
    "Correct type/symbol baselines",
]

FILES = [
    "moduleResolution/a.ts",
    "moduleResolution/bundler/b.ts|@module: esnext",
    "importAssertion/c.ts",
    "importAttributes/d.ts",
    "importDefer/e.ts",
    "nonjsExtensions/f.ts",
    "node/g.ts",
    "node/allowJs/h.ts",
    "externalModules/i.ts",
    "externalModules/typeOnly/j.ts",
    "internalModules/DeclarationMerging/k.ts",
    "dynamicImport/l.ts",
    "nodeLike/w.ts",
    "types/t.ts",
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
    def test_issue_row_selects_the_nine_directories_only(self):
        got = sorted(s.base for s in pick("issue1283"))
        self.assertEqual(got, [f"{c}.ts" for c in "abcdefghijkl"])

    def test_chunks_partition_the_group(self):
        seen = []
        for name in group.CHUNKS:
            seen += [s.key for s in pick(name)]
        self.assertEqual(sorted(seen), sorted(s.key for s in pick("issue1283")))

    def test_chunks_hold_their_directories(self):
        self.assertEqual(
            sorted(s.base for s in pick("mr")),
            ["a.ts", "b.ts", "c.ts", "d.ts", "e.ts", "f.ts"],
        )
        self.assertEqual(sorted(s.base for s in pick("node")), ["g.ts", "h.ts"])
        self.assertEqual(sorted(s.base for s in pick("ext")), ["i.ts", "j.ts"])
        self.assertEqual(sorted(s.base for s in pick("im")), ["k.ts", "l.ts"])

    def test_chunk_directories_are_the_group(self):
        named = [d for dirs in group.CHUNK_DIRS.values() for d in dirs]
        self.assertEqual(sorted(named), sorted(group.DIRS))
        self.assertEqual(len(set(named)), 9)

    def test_pins_come_from_the_oracle_json(self):
        table = {**conf.SLICES, **group.SLICES}
        self.assertEqual(conf.pinned_expected("issue1283", table), 5796)
        self.assertEqual(conf.pinned_expected("mr", table), 948)
        self.assertEqual(conf.pinned_expected("node", table), 2058)
        self.assertEqual(conf.pinned_expected("ext", table), 1818)
        self.assertEqual(conf.pinned_expected("im", table), 972)


if __name__ == "__main__":
    unittest.main()
