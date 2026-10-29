import importlib.util
import os
import re
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))
import ts_fourslash_control as control  # noqa: E402
import ts_fourslash_slices as fs  # noqa: E402

spec = importlib.util.spec_from_file_location(
    "run_ts_fourslash_oracle", SCRIPTS_DIR / "run-ts-fourslash-oracle.py"
)
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)


def entries(runner, *names):
    base = "tests/cases/fourslash" + ("/server" if runner.endswith("server") else "")
    out = []
    for n in names:
        key = f"{runner} tests {base}/{n}"
        its = [f"{runner} test {n} runs correctly"]
        if runner.endswith("server"):
            its.append(f"{runner} test {n} tsserver log")
        out += [(f"{key} {t}", t) for t in its]
    return out


NAMES = ("addX.ts", "Bar.ts", "baz.ts", "dEF.ts", "0num.ts", "zed.ts")
TESTS = fs.tests_from_titles(entries("fourslash", *NAMES), "fourslash")


class SelectTest(unittest.TestCase):
    def names(self, spec_text, table=None):
        return [t.base for t in fs.select(spec_text, TESTS, table or fs.SLICES)]

    def test_tests_keep_the_runners_order_and_one_title_per_file(self):
        self.assertEqual([t.base for t in TESTS], list(NAMES))
        self.assertEqual(
            TESTS[0].tests,
            [
                "fourslash tests tests/cases/fourslash/addX.ts fourslash test addX.ts runs correctly"
            ],
        )

    def test_server_files_have_a_log_test_and_other_runners_are_ignored(self):
        server = fs.tests_from_titles(
            entries("fourslash-server", "a.ts", "b.ts") + entries("fourslash", "c.ts"),
            "fourslash-server",
        )
        self.assertEqual([len(t.tests) for t in server], [2, 2])
        self.assertEqual(server[0].file, "tests/cases/fourslash/server/a.ts")
        self.assertEqual(fs.tests_from_titles(entries("fourslash", "c.ts"), "x"), [])

    def test_prefix_is_case_insensitive_and_rows_resolve(self):
        self.assertEqual(self.names("prefix:A-B"), ["addX.ts", "Bar.ts", "baz.ts"])
        self.assertEqual(self.names("prefix:0-9"), ["0num.ts"])
        self.assertEqual(self.names("a-c"), ["addX.ts", "Bar.ts", "baz.ts", "0num.ts"])
        self.assertEqual(self.names("d-g"), ["dEF.ts"])
        self.assertEqual(self.names("s-z"), ["zed.ts"])
        self.assertEqual(self.names("first200"), list(NAMES))

    def test_named_ranges_partition_the_names(self):
        rows = ("a-c", "d-g", "h-m", "n-r", "s-z")
        got = sorted(n for r in rows for n in self.names(r))
        self.assertEqual(got, sorted(NAMES))
        self.assertEqual(sorted(self.names("all")), sorted(NAMES))

    def test_first_shard_files_match_and_union(self):
        self.assertEqual(self.names("first:2"), ["addX.ts", "Bar.ts"])
        self.assertEqual(self.names("shard:2/3"), ["Bar.ts", "0num.ts"])
        self.assertEqual(self.names("files:baz.ts,zed.ts"), ["baz.ts", "zed.ts"])
        self.assertEqual(self.names("match:dEF"), ["dEF.ts"])
        self.assertEqual(self.names("first:1+files:zed.ts"), ["addX.ts", "zed.ts"])

    def test_extra_rows_only_add_to_the_table(self):
        table = dict(fs.SLICES, mine="prefix:z+first:1")
        self.assertEqual(self.names("mine", table), ["addX.ts", "zed.ts"])

    def test_bad_specs_raise(self):
        for bad in ("nope", "shard:4/3", "prefix:ab", "first:x"):
            with self.assertRaises(ValueError):
                fs.select(bad, TESTS, fs.SLICES)

    def test_grep_selects_exactly_the_tests_of_a_batch(self):
        server = fs.tests_from_titles(
            entries("fourslash-server", "a.ts", "ab.ts"), "fourslash-server"
        )
        grep = fs.grep_for([server[0]], "fourslash-server")
        titles = [t for s in server for t in s.tests]
        hits = [t for t in titles if re.search(grep.replace("\\/", "/"), t)]
        self.assertEqual(hits, server[0].tests)
        grep = fs.grep_for([TESTS[0]], "fourslash")
        self.assertTrue(re.search(grep.replace("\\/", "/"), TESTS[0].tests[0]))
        self.assertFalse(re.search(grep.replace("\\/", "/"), TESTS[1].tests[0]))

    def test_batches(self):
        self.assertEqual(len(fs.batches(TESTS, 4)), 2)


class ControlTest(unittest.TestCase):
    def tree(self, root):
        ref = Path(root, "ts", "tests", "baselines", "reference")
        (ref / "tsserver" / "fourslashServer").mkdir(parents=True)
        cases = Path(root, "ts", "tests", "cases", "fourslash")
        (cases / "server").mkdir(parents=True)
        (cases / "fourslash.ts").write_text("declare var verify: any;\n")
        return Path(root, "ts"), ref, cases

    def test_baseline_and_assertion_controls_edit_private_copies_only(self):
        with tempfile.TemporaryDirectory() as root:
            ts, ref, cases = self.tree(root)
            for n in ("a", "b", "c", "d", "e"):
                (cases / f"{n}.ts").write_text(f"// {n}\nverify.x();\n")
            for n, ext in (
                ("a", ".baseline"),
                ("b", ".baseline.jsonc"),
                ("c", ".baseline"),
            ):
                (ref / f"{n}{ext}").write_text("ref\n")
            (ref / "ab.baseline").write_text("neighbour\n")
            tests = fs.tests_from_titles(
                entries("fourslash", *(f"{n}.ts" for n in "abcde")), "fourslash"
            )
            work = Path(root, "work")
            work.mkdir()
            picked, rdir, cdir, done = control.make_control(
                str(ts), tests[:3], tests[3:], str(work)
            )
            self.assertEqual([t.base for t in picked], ["a.ts", "b.ts", "c.ts"])
            self.assertEqual(
                done,
                [
                    "a.baseline: append",
                    "b.baseline.jsonc: replace",
                    "c.baseline: remove",
                ],
            )
            self.assertTrue(
                Path(rdir, "a.baseline").read_text().endswith("corrupted reference\n")
            )
            self.assertFalse(Path(rdir, "c.baseline").exists())
            self.assertEqual((ref / "a.baseline").read_text(), "ref\n")
            self.assertFalse(Path(rdir, "ab.baseline").exists())
            self.assertEqual(
                Path(cdir, "fourslash", "a.ts").read_text(), "// a\nverify.x();\n"
            )

    def test_assertions_are_appended_when_a_slice_has_no_baseline(self):
        with tempfile.TemporaryDirectory() as root:
            ts, ref, cases = self.tree(root)
            for n in ("a", "b"):
                (cases / f"{n}.ts").write_text(f"// {n}\n")
            tests = fs.tests_from_titles(
                entries("fourslash", "a.ts", "b.ts"), "fourslash"
            )
            work = Path(root, "work")
            work.mkdir()
            picked, rdir, cdir, done = control.make_control(
                str(ts), tests, [], str(work)
            )
            self.assertEqual(len(picked), 2)
            self.assertIn(
                "verify.numberOfErrorsInCurrentFile",
                Path(cdir, "fourslash", "a.ts").read_text(),
            )
            self.assertIn(
                "verify.currentFileContentIs",
                Path(cdir, "fourslash", "b.ts").read_text(),
            )
            self.assertTrue(Path(cdir, "fourslash", "fourslash.ts").is_symlink())

    def test_server_tests_keep_their_directory_and_log_baseline(self):
        with tempfile.TemporaryDirectory() as root:
            ts, ref, cases = self.tree(root)
            (cases / "server" / "s.ts").write_text("// s\n")
            (ref / "tsserver" / "fourslashServer" / "s.js").write_text("log\n")
            tests = fs.tests_from_titles(
                entries("fourslash-server", "s.ts"), "fourslash-server"
            )
            work = Path(root, "work")
            work.mkdir()
            picked, rdir, cdir, done = control.make_control(
                str(ts), tests, [], str(work)
            )
            self.assertEqual(
                done, [os.path.join("tsserver", "fourslashServer", "s.js") + ": append"]
            )
            self.assertTrue(Path(cdir, "fourslash", "server", "s.ts").exists())


class OracleTest(unittest.TestCase):
    def test_pinned_counts_follow_the_runner_buckets(self):
        table = dict(fs.SLICES)
        n = oracle.compiler.pinned_expected("a-c", table, "fourslash")
        self.assertIsInstance(n, int)
        self.assertEqual(
            oracle.compiler.pinned_expected("first200", table, "fourslash"), None
        )

    def test_the_default_known_failures_file_exists_and_parses(self):
        known = oracle.load_known(oracle.KNOWN)
        self.assertIsInstance(known, dict)

    def test_checkout_problem_without_a_checkout(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertIn(
                "not a built TypeScript", oracle.checkout_problem(d, "fourslash")
            )


if __name__ == "__main__":
    unittest.main()
