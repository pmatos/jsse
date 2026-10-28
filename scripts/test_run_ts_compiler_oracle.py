import collections
import importlib.util
import os
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))
import ts_compiler_slices as slices  # noqa: E402

spec = importlib.util.spec_from_file_location(
    "run_ts_compiler_oracle", SCRIPTS_DIR / "run-ts-compiler-oracle.py"
)
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)

ITS = [
    "Correct errors",
    "Correct module resolution tracing",
    "Correct sourcemap content",
    "Correct JS output",
    "Correct Sourcemap output",
    "Correct type/symbol baselines",
]


def entries(*files):
    out = []
    for f in files:
        name, _, cfg = f.partition("|")
        suite = f"compiler tests compiler tests for tests/cases/compiler/{name}"
        if cfg:
            suite += f" ({cfg})"
        for it in ITS:
            out.append((f"{suite} {it} for {name}", f"{it} for {name}"))
    return out


SUITES = slices.suites_from_titles(
    entries(
        "2dArrays.ts",
        "APILibCheck.ts",
        "b.ts|target=es5",
        "b.ts|target=es2015",
        "Cc.tsx",
        "d.ts",
        "E.ts",
    )
)


def res(*outcomes, errors=None, local=None):
    r = {
        "outcomes": collections.Counter(),
        "errors": errors or {},
        "local": local or {},
    }
    for title, kind in outcomes:
        r["outcomes"][(title, kind)] += 1
    return r


class SelectTest(unittest.TestCase):
    def names(self, spec_text, table=None):
        return [(s.base, s.config) for s in slices.select(spec_text, SUITES, table)]

    def test_suites_group_configurations_and_keep_order(self):
        self.assertEqual(len(SUITES), 7)
        self.assertEqual(SUITES[2].config, "target=es5")
        self.assertEqual(len(SUITES[0].tests), 6)

    def test_first_prefix_shard_files_and_match(self):
        self.assertEqual(len(slices.select("first:2", SUITES)), 2)
        self.assertEqual(self.names("prefix:0-9"), [("2dArrays.ts", None)])
        self.assertEqual(
            [b for b, _ in self.names("prefix:a-b")], ["APILibCheck.ts", "b.ts", "b.ts"]
        )
        self.assertEqual(
            [b for b, _ in self.names("shard:2/3")],
            [
                "APILibCheck.ts",
                "Cc.tsx",
            ],
        )
        self.assertEqual(len(self.names("files:b.ts")), 2)
        self.assertEqual(self.names("match:es2015"), [("b.ts", "target=es2015")])

    def test_union_and_named_rows(self):
        self.assertEqual(len(slices.select("first:1+files:d.ts", SUITES)), 2)
        table = {"x": "prefix:e", "y": "x+first:1"}
        self.assertEqual(
            [b for b, _ in self.names("y", table)], ["2dArrays.ts", "E.ts"]
        )

    def test_match_takes_the_rest_of_the_spec_plus_signs_included(self):
        self.assertEqual(
            slices.split_terms("first:1+match:a+b|c+d"), ["first:1", "match:a+b|c+d"]
        )
        self.assertEqual(slices.split_terms("match:x"), ["match:x"])
        self.assertEqual(
            slices.select("match:Arrays\\.t.+$", SUITES)[0].base, "2dArrays.ts"
        )

    def test_a_row_naming_itself_is_an_error_not_a_hang(self):
        with self.assertRaises(ValueError):
            slices.flat_terms("x", {"x": "x+first:1"})
        with self.assertRaises(ValueError):
            slices.flat_terms("x", {"x": "y", "y": "x"})
        self.assertEqual(
            slices.flat_terms("x", {"x": "first:1+prefix:a"}), ["first:1", "prefix:a"]
        )

    def test_repeated_titles_are_not_counted_twice(self):
        doubled = slices.suites_from_titles(entries("d.ts") * 2)
        self.assertEqual(len(doubled[0].tests), 6)

    def test_bad_specs_raise(self):
        for bad in ("nope", "shard:4/3", "prefix:ab", "first:x"):
            with self.assertRaises(ValueError):
                slices.select(bad, SUITES)

    def test_default_table_rows_resolve(self):
        for name in slices.SLICES:
            slices.select(name, SUITES)

    def test_grep_selects_exactly_the_suites_of_a_batch(self):
        import re

        grep = slices.grep_for([SUITES[2]])
        hits = [
            t for s in SUITES for t in s.tests if re.search(grep.replace("\\/", "/"), t)
        ]
        self.assertEqual(hits, SUITES[2].tests)
        self.assertEqual(len(slices.batches(SUITES, 3)), 3)


class ClassifyTest(unittest.TestCase):
    def test_equal_runs_have_nothing_to_report(self):
        a = res(("t1", "passing"), ("t2", "pending"))
        self.assertEqual(oracle.classify(a, a, {}), [])

    def test_unlisted_jsse_failure_is_reported_and_listed_one_is_not(self):
        node = res(("t1", "passing"))
        jsse = res(("t1", "failing"), errors={"t1": "diff"})
        self.assertIn("not in the baseline", oracle.classify(node, jsse, {})[0])
        self.assertEqual(oracle.classify(node, jsse, {"t1": "#9"}), [])

    def test_stale_known_failure_and_node_failure(self):
        a = res(("t1", "passing"))
        self.assertIn("no longer fails", oracle.classify(a, a, {"t1": "#9"})[0])
        b = res(("t1", "failing"), errors={"t1": "m"})
        self.assertEqual(oracle.classify(b, b, {}), ["fails on Node itself: t1"])

    def test_both_failing_with_different_messages(self):
        node = res(("t1", "failing"), errors={"t1": "a"})
        jsse = res(("t1", "failing"), errors={"t1": "b"})
        lines = oracle.classify(node, jsse, {})
        self.assertTrue(any("different messages" in x for x in lines))

    def test_test_not_run_on_jsse(self):
        node = res(("t1", "passing"), ("t2", "passing"))
        jsse = res(("t1", "passing"))
        self.assertIn("t2", oracle.classify(node, jsse, {})[0])


class LocalTest(unittest.TestCase):
    def test_local_baselines_compared_byte_for_byte(self):
        with tempfile.TemporaryDirectory() as ref:
            Path(ref, "same.js").write_bytes(b"x")
            node = res(local={"a.js": b"1", "b.js": b"2", "same.js": b"x"})
            jsse = res(
                local={"a.js": b"1", "b.js": b"3", "c.js": b"4", "same.js": b"x"}
            )
            text = "\n".join(oracle.local_problems(node, jsse, ref))
        self.assertIn("b.js differs", text)
        self.assertIn("c.js only written by jsse", text)
        self.assertIn("same.js equals its reference", text)
        self.assertNotIn("a.js", text)


class RecordTest(unittest.TestCase):
    def test_only_known_failure_problems_are_resolved_by_recording(self):
        problems = [
            "passes on Node, fails on jsse (not in the baseline): t",
            "known-failures file lists a test that no longer fails on jsse: u",
            "local baseline x differs",
        ]
        self.assertEqual(oracle.recordable(problems), ["local baseline x differs"])


class ControlTest(unittest.TestCase):
    def test_corrupts_the_suites_own_files_not_a_dotted_neighbour(self):
        import ts_compiler_control as control

        with (
            tempfile.TemporaryDirectory() as ref,
            tempfile.TemporaryDirectory() as dest,
        ):
            for n in ("foo.js", "foo.types", "foo.bar.js", "foo.bar.types"):
                Path(ref, n).write_text("ref\n")
            suite = slices.Suite(
                "compiler tests compiler tests for tests/cases/compiler/foo.ts", []
            )
            picked, done = control.make_control(ref, [suite], dest)
            self.assertEqual(done, ["foo.js: append"])
            self.assertTrue(
                Path(dest, "foo.js").read_text().endswith("corrupted reference\n")
            )
            self.assertEqual(Path(dest, "foo.bar.js").read_text(), "ref\n")


class KnownFileTest(unittest.TestCase):
    def test_roundtrip_and_comments(self):
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "known.txt")
            oracle.write_known(path, {"b t": "#2", "a t": "#1"})
            self.assertEqual(oracle.load_known(path), {"a t": "#1", "b t": "#2"})
            self.assertEqual(oracle.load_known(os.path.join(d, "none")), {})


class MergeTest(unittest.TestCase):
    def test_outcomes_of_json_report(self):
        doc = {
            "passes": [{"fullTitle": "p"}],
            "failures": [{"fullTitle": "f", "err": {"message": " boom \n"}}],
            "pending": [],
        }
        outcomes, errors = oracle.outcomes_of(doc)
        self.assertEqual(
            outcomes, collections.Counter({("p", "passing"): 1, ("f", "failing"): 1})
        )
        self.assertEqual(errors, {"f": "boom"})
        merged = oracle.merge(
            [
                {
                    "report": doc,
                    "secs": 1.0,
                    "local": {"x": b"1"},
                    "exit": "0",
                    "tail": "",
                }
            ]
        )
        self.assertEqual(
            oracle.counts(merged), {"passing": 1, "failing": 1, "pending": 0}
        )

    def test_missing_report_is_recorded(self):
        merged = oracle.merge(
            [{"report": None, "secs": 1.0, "local": {}, "exit": "timeout", "tail": "t"}]
        )
        self.assertIn("timeout", merged["bad"][0])


if __name__ == "__main__":
    unittest.main()
