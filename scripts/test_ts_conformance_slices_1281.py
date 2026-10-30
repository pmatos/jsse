import importlib.util
import sys
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))
import ts_compiler_slices as compiler_slices  # noqa: E402
import ts_conformance_slices_1281 as conf  # noqa: E402

wrapper_spec = importlib.util.spec_from_file_location(
    "run_ts_conformance_1281_oracle", SCRIPTS_DIR / "run-ts-conformance-1281-oracle.py"
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
        "declarationEmit/a.ts",
        "declarationEmit/typePredicates/p.ts",
        "emitter/es5/e.ts",
        "emitter/es2015/e.ts|@target: es2015",
        "ambient/amb.ts",
        "additionalChecks/n.ts",
        "ambientX/other.ts",
        "classes/c.ts",
        "types/t.ts",
    )
)


class Conformance1281Test(unittest.TestCase):
    def files(self, spec):
        return [s.rel for s in conf.select(spec, SUITES)]

    def test_group_rows_select_their_directories(self):
        self.assertEqual(
            self.files("declarationEmit"),
            ["declarationEmit/a.ts", "declarationEmit/typePredicates/p.ts"],
        )
        self.assertEqual(
            self.files("emitter"), ["emitter/es5/e.ts", "emitter/es2015/e.ts"]
        )
        self.assertEqual(self.files("ambient"), ["ambient/amb.ts"])
        self.assertEqual(self.files("additionalChecks"), ["additionalChecks/n.ts"])

    def test_issue_row_is_the_union_of_the_four_directories(self):
        whole = sorted(s.key for s in conf.select("issue1281", SUITES))
        seen = []
        for d in conf.GROUP_1281:
            seen += [s.key for s in conf.select(d, SUITES)]
        self.assertEqual(sorted(seen), whole)
        self.assertEqual(len(whole), 6)

    def test_other_terms_delegate(self):
        self.assertEqual(len(conf.select("first:2", SUITES)), 2)
        self.assertEqual(
            self.files("dir:emitter/es5+files:amb.ts"),
            ["emitter/es5/e.ts", "ambient/amb.ts"],
        )

    def test_pinned_counts_come_from_the_oracle_json(self):
        table = conf.SLICES
        dirs = conf.top_dirs("issue1281", table)
        self.assertEqual(sorted(dirs), sorted(conf.GROUP_1281))
        base = wrapper.base
        self.assertEqual(base.pinned_expected("issue1281", table), 378)
        self.assertEqual(base.pinned_expected("declarationEmit", table), 138)
        self.assertEqual(base.pinned_expected("emitter", table), 102)
        self.assertEqual(base.pinned_expected("ambient", table), 132)
        self.assertEqual(base.pinned_expected("additionalChecks", table), 6)
        self.assertIsNone(base.pinned_expected("first:3", table))

    def test_wrapper_points_the_oracle_at_this_group(self):
        wrapper.base.configure()
        oracle = wrapper.base.compiler
        self.assertIs(oracle.slices, conf)
        self.assertTrue(wrapper.KNOWN.endswith("conformance-1281-known-failures.txt"))
        self.assertEqual(oracle.CONFIG["runners"], ["conformance"])


if __name__ == "__main__":
    unittest.main()
