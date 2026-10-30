import importlib.util
import os
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))
import ts_conformance_control_1284 as control  # noqa: E402
import ts_conformance_slices_1275 as conf  # noqa: E402
import ts_conformance_slices_1284 as group  # noqa: E402

_spec = importlib.util.spec_from_file_location(
    "run_ts_conformance_1284_oracle", SCRIPTS_DIR / "run-ts-conformance-1284-oracle.py"
)
wrapper = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(wrapper)

ITS = [
    "Correct errors",
    "Correct module resolution tracing",
    "Correct sourcemap content",
    "Correct JS output",
    "Correct Sourcemap output",
    "Correct type/symbol baselines",
]

FILES = [
    "async/asyncFunctionDeclarationParameterEvaluation.ts|@target: es2015",
    "async/es5/a.ts",
    "async/es2017/b.ts",
    "async/es6/c.ts|@target: es5",
    "asyncGenerators/d.ts",
    "generators/e.ts",
    "enums/f.ts",
    "constEnums/g.ts",
    "interfaces/declarationMerging/h.ts",
    "interfaces/interfaceDeclarations/i.ts",
    "interfaces/interfacesExtendingClasses/j.ts",
    "Symbols/k.ts",
    "fixSignatureCaching.ts",
    "inferFromBindingPattern.ts",
    "asynchronous/l.ts",
    "classes/m.ts",
    "es6/n.ts",
    "types/o.ts",
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
    def test_issue_row_selects_the_group_only(self):
        got = sorted(s.base for s in pick("issue1284"))
        self.assertEqual(
            got,
            sorted(
                [
                    "asyncFunctionDeclarationParameterEvaluation.ts",
                    "a.ts",
                    "b.ts",
                    "c.ts",
                    "d.ts",
                    "e.ts",
                    "f.ts",
                    "g.ts",
                    "h.ts",
                    "i.ts",
                    "j.ts",
                    "k.ts",
                    "fixSignatureCaching.ts",
                    "inferFromBindingPattern.ts",
                ]
            ),
        )

    def test_chunks_partition_the_group(self):
        seen = []
        for name in group.CHUNKS:
            seen += [s.key for s in pick(name)]
        self.assertEqual(sorted(seen), sorted(s.key for s in pick("issue1284")))
        self.assertEqual(len(seen), len(set(seen)))

    def test_chunks_split_async_and_interfaces_by_directory(self):
        self.assertEqual([s.base for s in pick("as1")], ["a.ts"])
        self.assertEqual(sorted(s.base for s in pick("as2")), ["b.ts", "c.ts"])
        self.assertEqual(sorted(s.base for s in pick("if")), ["h.ts", "i.ts", "j.ts"])
        self.assertEqual(
            sorted(s.base for s in pick("mix")),
            sorted(
                [
                    "asyncFunctionDeclarationParameterEvaluation.ts",
                    "d.ts",
                    "e.ts",
                    "f.ts",
                    "g.ts",
                    "k.ts",
                    "fixSignatureCaching.ts",
                    "inferFromBindingPattern.ts",
                ]
            ),
        )

    def test_directory_rows_select_by_whole_directory_name(self):
        self.assertEqual(
            sorted(s.base for s in pick("async")),
            [
                "a.ts",
                "asyncFunctionDeclarationParameterEvaluation.ts",
                "b.ts",
                "c.ts",
            ],
        )
        self.assertEqual([s.base for s in pick("enums")], ["f.ts"])

    def test_pins_come_from_the_oracle_json(self):
        table = {**conf.SLICES, **group.SLICES}
        self.assertEqual(conf.pinned_expected("async", table), 1512)
        self.assertEqual(conf.pinned_expected("interfaces", table), 396)
        self.assertEqual(conf.pinned_expected("Symbols", table), 96)
        pin, buckets, loose = wrapper.pinned_tests()
        self.assertEqual((pin, loose), (2286, 12))
        self.assertEqual(len(buckets), 52)


class Owners(unittest.TestCase):
    def setUp(self):
        _, self.buckets, self.loose = wrapper.pinned_tests()
        names = [n for ns in group.OWNERS.values() for n in ns]
        self.entries = list(names)

    def test_every_oracle_bucket_has_exactly_one_owner(self):
        self.assertEqual(
            wrapper.owner_problems(self.entries, self.buckets, self.loose), []
        )
        owned = [n for ns in group.OWNERS.values() for n in ns if not n.endswith(".ts")]
        self.assertEqual(sorted(owned), sorted(self.buckets))

    def test_a_directory_run_by_two_issues_is_reported(self):
        group.OWNERS[1285] = ["types"]
        try:
            bad = wrapper.owner_problems(self.entries, self.buckets, self.loose)
        finally:
            del group.OWNERS[1285]
        self.assertEqual(bad, ["types is run by #1274 and #1285"])

    def test_a_new_directory_run_by_no_issue_is_reported(self):
        bad = wrapper.owner_problems(
            [*self.entries, "newDir"], self.buckets, self.loose
        )
        self.assertEqual(bad, ["on disk, run by no issue: newDir"])

    def test_a_missing_directory_is_reported(self):
        entries = [e for e in self.entries if e != "salsa"]
        bad = wrapper.owner_problems(entries, self.buckets, self.loose)
        self.assertEqual(bad, ["owned but not on disk: salsa"])

    def test_root_file_count_must_match_the_unbucketed_pin(self):
        bad = wrapper.owner_problems(self.entries, self.buckets, self.loose + 6)
        self.assertEqual(len(bad), 1)
        self.assertIn("root files", bad[0])


REFS = [
    "a(target=es2015).js",
    "a(target=es2015).symbols",
    "a(target=es2015).types",
    "a(target=es5).errors.txt",
    "a(target=es5).js",
    "a(target=es5).symbols",
    "a(target=es5).types",
    "a_es5(target=es5).js",
    "b.js",
    "b.symbols",
    "b.types",
]


def suite_of(base, config):
    key = (
        f"conformance tests conformance tests for tests/cases/conformance/async/{base}"
    )
    key += f" ({config})" if config else ""
    return conf.Suite(key, [])


class Control(unittest.TestCase):
    def test_options_match_the_spelling_of_baseline_file_names(self):
        self.assertEqual(control.options("@target: ES5"), {"target=es5"})
        self.assertEqual(
            control.options("@target: es2015, @strict: true"),
            {"target=es2015", "strict=true"},
        )
        self.assertEqual(control.options(None), set())

    def test_baselines_are_those_of_the_suites_configuration_only(self):
        es5 = control.baselines_of(suite_of("a.ts", "@target: es5"), REFS)
        self.assertEqual(
            es5,
            [
                "a(target=es5).errors.txt",
                "a(target=es5).js",
                "a(target=es5).symbols",
                "a(target=es5).types",
            ],
        )
        es2015 = control.baselines_of(suite_of("a.ts", "@target: es2015"), REFS)
        self.assertEqual(len(es2015), 3)
        self.assertTrue(all("es2015" in n for n in es2015))
        self.assertEqual(
            control.baselines_of(suite_of("b.ts", None), REFS),
            ["b.js", "b.symbols", "b.types"],
        )

    def test_the_corrupted_file_belongs_to_the_picked_suite(self):
        with (
            tempfile.TemporaryDirectory() as ref,
            tempfile.TemporaryDirectory() as dest,
        ):
            for n in REFS:
                with open(os.path.join(ref, n), "w") as f:
                    f.write("x\n")
            chosen = [
                suite_of("a.ts", "@target: es5"),
                suite_of("a.ts", "@target: es2015"),
            ]
            picked, done = control.make_control(ref, chosen, dest)
            self.assertEqual(
                [s.config for s in picked], ["@target: es5", "@target: es2015"]
            )
            self.assertEqual(done[0], "a(target=es5).js: append")
            self.assertEqual(done[1], "a(target=es2015).types: append")
            self.assertTrue(
                os.path.exists(os.path.join(dest, "a(target=es2015).types"))
            )

    def test_an_added_reference_keeps_the_configuration_in_its_name(self):
        with (
            tempfile.TemporaryDirectory() as ref,
            tempfile.TemporaryDirectory() as dest,
        ):
            for n in REFS:
                with open(os.path.join(ref, n), "w") as f:
                    f.write("x\n")
            chosen = [suite_of("a.ts", "@target: es2015")] * 1
            others = [suite_of(f"c{i}.ts", None) for i in range(6)]
            for i in range(6):
                for e in (".js", ".types"):
                    with open(os.path.join(ref, f"c{i}{e}"), "w") as f:
                        f.write("x\n")
            picked, done = control.make_control(ref, chosen + others, dest)
            self.assertEqual(len(picked), 7)
            self.assertTrue(done[-1].endswith(".errors.txt: add"), done)


if __name__ == "__main__":
    unittest.main()
