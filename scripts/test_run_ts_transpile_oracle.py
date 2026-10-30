import importlib.util
import os
import re
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))
import ts_transpile_control as control  # noqa: E402
import ts_transpile_corpus as corpus  # noqa: E402
import ts_transpile_slices as ts  # noqa: E402

spec = importlib.util.spec_from_file_location(
    "run_ts_transpile_oracle", SCRIPTS_DIR / "run-ts-transpile-oracle.py"
)
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)


def entries(*specs):
    """specs: (file name, test name, kinds) -> (fullTitle, title) pairs as the mocha dry run lists them."""
    out = []
    for name, test, kinds in specs:
        key = f"transpile tests tests/cases/transpile/{name}"
        for k in kinds:
            title = f"transpile test {test} has expected {k} output"
            out.append((f"{key} {title}", title))
    return out


SPECS = (
    ("declA.ts", "declA", ("js", "declaration")),
    ("declB.ts", "declB(declarationMap=true)", ("js", "declaration")),
    ("declB.ts", "declB(declarationMap=false)", ("js", "declaration")),
    ("jsWith.ts", "jsWith", ("js",)),
    ("Zed.ts", "Zed", ("declaration",)),
)
TESTS = ts.tests_from_titles(entries(*SPECS))


class SelectTest(unittest.TestCase):
    def names(self, spec_text, table=None):
        return [t.base for t in ts.select(spec_text, TESTS, table or ts.SLICES)]

    def test_a_file_groups_all_its_configurations_in_the_runners_order(self):
        self.assertEqual(
            [t.base for t in TESTS], ["declA.ts", "declB.ts", "jsWith.ts", "Zed.ts"]
        )
        self.assertEqual([len(t.tests) for t in TESTS], [2, 4, 1, 1])

    def test_other_runners_and_foreign_titles_are_ignored(self):
        other = [
            (
                "compiler tests tests/cases/x.ts t",
                "t",
            ),
            ("transpile tests nothing", "nothing"),
        ]
        self.assertEqual(ts.tests_from_titles(other), [])

    def test_rows_and_terms(self):
        self.assertEqual(self.names("all"), [t.base for t in TESTS])
        self.assertEqual(self.names("declaration"), ["declA.ts", "declB.ts"])
        self.assertEqual(self.names("first:1"), ["declA.ts"])
        self.assertEqual(self.names("shard:2/2"), ["declB.ts", "Zed.ts"])
        self.assertEqual(self.names("files:jsWith.ts"), ["jsWith.ts"])
        self.assertEqual(self.names("match:declB"), ["declB.ts"])
        self.assertEqual(self.names("prefix:z+first:1"), ["declA.ts", "Zed.ts"])

    def test_bad_specs_raise(self):
        for bad in ("nope", "shard:4/3", "prefix:ab", "first:x"):
            with self.assertRaises(ValueError):
                ts.select(bad, TESTS, ts.SLICES)

    def test_grep_selects_exactly_the_tests_of_a_batch(self):
        grep = ts.grep_for([TESTS[1]]).replace("\\/", "/")
        titles = [t for s in TESTS for t in s.tests]
        hits = [t for t in titles if re.search(grep, t)]
        self.assertEqual(hits, TESTS[1].tests)

    def test_batches_never_split_a_file(self):
        sizes = [[len(t.tests) for t in b] for b in ts.batches_by_tests(TESTS, 4)]
        self.assertEqual(sizes, [[2], [4], [1, 1]])
        self.assertEqual(ts.batches_by_tests([], 4), [])

    def test_pinned_count_only_for_the_whole_runner(self):
        table = dict(ts.SLICES)
        self.assertEqual(ts.pinned_expected("all", table, oracle.compiler.SUITES), 37)
        self.assertEqual(
            ts.pinned_expected("first:100000", table, oracle.compiler.SUITES), 37
        )
        self.assertIsNone(ts.pinned_expected("first10", table, oracle.compiler.SUITES))

    def test_reference_names(self):
        self.assertEqual(
            ts.reference_names("transpile test a(b=true) has expected js output"),
            "a(b=true).js",
        )
        self.assertEqual(
            ts.reference_names("transpile test a has expected declaration output"),
            "a.d.ts",
        )
        self.assertIsNone(ts.reference_names("something else"))


class ControlTest(unittest.TestCase):
    def tree(self, root, names):
        tsd = Path(root, "ts")
        ref = tsd / ts.REFERENCE
        cases = tsd / ts.CASES
        ref.mkdir(parents=True)
        cases.mkdir(parents=True)
        for n in names:
            (cases / f"{n}.ts").write_bytes(f"// {n}\r\nlet x = 1;\r\n".encode())
            (ref / f"{n}.js").write_text("ref js\n")
            (ref / f"{n}.d.ts").write_text("ref d.ts\n")
        return tsd, ref

    def mk(self, names):
        return ts.tests_from_titles(
            entries(*((f"{n}.ts", n, ("js", "declaration")) for n in names))
        )

    def test_baseline_and_input_controls_edit_private_copies_only(self):
        names = "abcdef"
        with tempfile.TemporaryDirectory() as root:
            tsd, ref = self.tree(root, names)
            tests = self.mk(names)
            work = Path(root, "work")
            work.mkdir()
            picked, rdir, cdir, done = control.make_control(
                str(tsd), tests[:2], tests[2:], str(work)
            )
            self.assertEqual(
                [t.base for t in picked], ["a.ts", "b.ts", "c.ts", "d.ts", "e.ts"]
            )
            self.assertEqual(
                done[:3], ["a.js: append", "b.js: replace", "c.js: remove"]
            )
            self.assertIn("export const corrupted", done[3])
            self.assertIn("export function corrupted", done[4])
            r = Path(rdir, "transpile")
            self.assertTrue((r / "a.js").read_text().endswith("corrupted reference\n"))
            self.assertEqual((r / "a.d.ts").read_text(), "ref d.ts\n")
            self.assertFalse((r / "c.js").exists())
            self.assertEqual((r / "d.js").read_text(), "ref js\n")
            self.assertFalse((r / "f.js").exists())
            self.assertEqual((ref / "a.js").read_text(), "ref js\n")
            c = Path(cdir, "transpile")
            self.assertEqual((c / "a.ts").read_bytes(), b"// a\r\nlet x = 1;\r\n")
            self.assertTrue((c / "d.ts").read_text().endswith("corrupted = 12345;\n"))
            self.assertFalse((c / "f.ts").exists())

    def test_files_without_a_reference_are_skipped(self):
        with tempfile.TemporaryDirectory() as root:
            tsd, ref = self.tree(root, "ab")
            (ref / "a.js").unlink()
            (ref / "a.d.ts").unlink()
            work = Path(root, "work")
            work.mkdir()
            picked, _, _, _ = control.make_control(
                str(tsd), self.mk("ab"), [], str(work)
            )
            self.assertEqual([t.base for t in picked], ["b.ts"])


class CorpusTest(unittest.TestCase):
    def tree(self, root):
        for d, names in (
            ("transpile", ["t.ts"]),
            ("compiler", ["a.ts", "b.tsx", "c.js", "d.mts"]),
            ("conformance/x", ["e.cts", "f.d.ts"]),
        ):
            p = Path(root, "tests", "cases", d)
            p.mkdir(parents=True)
            for n in names:
                (p / n).write_text("")

    def test_corpus_files_are_sorted_typescript_tests_and_strided_evenly(self):
        with tempfile.TemporaryDirectory() as root:
            self.tree(root)
            everything = corpus.corpus_files(root, None)
            self.assertEqual(
                everything,
                [
                    "tests/cases/compiler/a.ts",
                    "tests/cases/compiler/b.tsx",
                    "tests/cases/compiler/d.mts",
                    "tests/cases/conformance/x/e.cts",
                    "tests/cases/conformance/x/f.d.ts",
                    "tests/cases/transpile/t.ts",
                ],
            )
            self.assertEqual(corpus.corpus_files(root, 100), everything)
            self.assertEqual(corpus.corpus_files(root, 3), everything[0::2])
            self.assertEqual(corpus.corpus_files(root, 0), [])

    def test_chunks_and_count(self):
        self.assertEqual(corpus.chunks(list(range(5)), 2), [[0, 1], [2, 3], [4]])
        self.assertIsNone(corpus.parse_count("all"))
        self.assertEqual(corpus.parse_count("12"), 12)

    def test_compare_reports_differences_and_missing_cases(self):
        a = {("f", "s"): "x\t1", ("g", "s"): "y\t1", ("h", "s"): "z\t1"}
        b = {("f", "s"): "x\t1", ("g", "s"): "other\t1", ("i", "s"): "w\t1"}
        got = corpus.compare(a, b)
        self.assertEqual(len(got), 3)
        self.assertIn("g [s] differs", got[0])
        self.assertIn("h [s] missing on jsse", got[1])
        self.assertIn("i [s] missing on Node", got[2])
        self.assertEqual(corpus.compare(a, a), [])
        many = {(str(i), "s"): "x" for i in range(30)}
        self.assertEqual(len(corpus.compare(many, {})), 21)

    def test_stack_depth_differences_are_exempt_only_for_the_known_deep_files(self):
        deep = "throws RangeError: Maximum call stack size exceeded\t0"
        known = ("tests/cases/compiler/binderBinaryExpressionStress.ts", "declaration")
        other = ("tests/cases/compiler/other.ts", "declaration")
        a = {known: "x\t1", other: "y\t1", ("g", "s"): "y\t1"}
        b = {known: deep, other: deep, ("g", "s"): "other\t1"}
        self.assertEqual(corpus.stack_limited(a, b), [known])
        self.assertEqual(corpus.stack_limited(b, a), [known])
        self.assertEqual(corpus.stack_limited(b, b), [])
        got = corpus.compare(a, b)
        self.assertEqual(len(got), 2)
        self.assertIn("g [s] differs", got[0])
        self.assertIn("other.ts [declaration] differs", got[1])

    def test_count_validation(self):
        with self.assertRaises(ValueError):
            corpus.parse_count("abc")

    def test_the_driver_exists_next_to_the_module(self):
        self.assertTrue(os.path.isfile(corpus.DRIVER))


class OracleTest(unittest.TestCase):
    def test_the_default_known_failures_file_exists_and_parses(self):
        self.assertTrue(os.path.exists(oracle.KNOWN))
        self.assertIsInstance(oracle.load_known(oracle.KNOWN), dict)

    def test_checkout_problem_without_a_checkout(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertIn("not a built TypeScript", oracle.checkout_problem(d))


if __name__ == "__main__":
    unittest.main()
