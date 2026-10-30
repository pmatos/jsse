import sys
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))
import ts_conformance_slices_1275 as conf  # noqa: E402
import ts_conformance_slices_1278 as group  # noqa: E402

TABLE = {**conf.SLICES, **group.SLICES}

ITS = [
    "Correct errors",
    "Correct module resolution tracing",
    "Correct sourcemap content",
    "Correct JS output",
    "Correct Sourcemap output",
    "Correct type/symbol baselines",
]

FILES = [
    "es5/a.ts",
    "es6/b.ts",
    "es7/exponentiationOperator/c.ts",
    "es7/exponentiationOperator/c.ts|@target: es5",
    "es2016/d.ts",
    "es2019/e.ts",
    "es2021/f.ts",
    "es2025/g.ts",
    "esDecorators/h.ts",
    "esnext/logicalAssignment/i.ts",
    "es20160/j.ts",
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


def bases(spec):
    return [s.base for s in conf.select(spec, suites(), TABLE)]


class Slices(unittest.TestCase):
    def test_group_is_the_thirteen_directories_of_the_issue(self):
        self.assertEqual(len(group.GROUP_1278), 13)
        self.assertEqual(group.GROUP_1278[:2], ["es5", "es7"])
        self.assertEqual(group.GROUP_1278[-1], "esnext")
        self.assertIn("es2016", group.GROUP_1278)
        self.assertIn("es2025", group.GROUP_1278)
        self.assertNotIn("es6", group.GROUP_1278)
        self.assertNotIn("esDecorators", group.GROUP_1278)

    def test_group_selects_only_its_directories_by_whole_name(self):
        self.assertEqual(
            bases("issue1278"),
            ["a.ts", "c.ts", "c.ts", "d.ts", "e.ts", "f.ts", "g.ts", "i.ts"],
        )

    def test_chunks_partition_the_group(self):
        whole = [s.key for s in conf.select("issue1278", suites(), TABLE)]
        seen = []
        for name in group.CHUNKS:
            seen += [s.key for s in conf.select(name, suites(), TABLE)]
        self.assertEqual(sorted(seen), sorted(whole))
        self.assertEqual(len(seen), len(set(seen)))

    def test_chunk_directories_are_the_group(self):
        named = [d for dirs in group.CHUNK_DIRS.values() for d in dirs]
        self.assertEqual(sorted(named), sorted(group.GROUP_1278))

    def test_pinned_counts_come_from_the_oracle_json(self):
        self.assertEqual(conf.pinned_expected("issue1278", TABLE), 1356)
        self.assertEqual(
            [conf.pinned_expected(c, TABLE) for c in group.CHUNKS], [414, 294, 444, 204]
        )
        self.assertEqual(conf.pinned_expected("es7", TABLE), 402)


if __name__ == "__main__":
    unittest.main()
