import collections
import json
import os
import re
import sys
import unittest

SCRIPTS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, SCRIPTS)
import ts_passlist as pl  # noqa: E402
import ts_passlist_scripted as scripted  # noqa: E402
import ts_passlist_store as store  # noqa: E402

CASE = "tests/cases/compiler/2dArrays.ts"
SUITE = f"compiler tests compiler tests for {CASE}"
UNIT = "unittests:: tsserver:: projectErrors:: with config file"


def compiler_entries(*names):
    out = []
    for n in names:
        for kind in ("errors", "JS output"):
            title = f"Correct {kind} for tests/cases/compiler/{n}"
            out.append(
                (
                    f"compiler tests compiler tests for tests/cases/compiler/{n} {title}",
                    title,
                )
            )
    return out


class IdTest(unittest.TestCase):
    def test_id_is_the_id_of_the_oracle_manifest(self):
        t = pl.Test(
            f"{SUITE} Correct JS output for {CASE}", f"Correct JS output for {CASE}"
        )
        self.assertEqual(t.id, f"{SUITE} :: Correct JS output for {CASE}")
        self.assertEqual((t.runner, t.bucket, t.parent), ("compiler", "2", SUITE))

    def test_root_level_test_has_an_empty_parent(self):
        title = "unittests:: tsbuildWatch:: watchMode:: publicAPI:: Public API with custom transformers"
        t = pl.Test(title, title)
        self.assertEqual(t.id, " :: " + title)
        self.assertEqual((t.runner, t.bucket), ("unittests", "tsbuildWatch"))

    def test_buckets_per_runner(self):
        cases = {
            (
                "conformance tests conformance tests for tests/cases/conformance/types/a/b.ts t",
                "t",
            ): ("conformance", "types"),
            (
                "conformance tests conformance tests for tests/cases/conformance/root.ts t",
                "t",
            ): ("conformance", "(root)"),
            (
                "fourslash tests tests/cases/fourslash/zed.ts fourslash test zed.ts runs correctly",
                "fourslash test zed.ts runs correctly",
            ): ("fourslash", "Z"),
            (
                "projects tests Compiling project for x: testcase tests/cases/project/mapRootRelativePath.json (@module: amd) t",
                "t",
            ): ("project", "map"),
            (
                "transpile tests tests/cases/transpile/a.ts transpile test a has expected js output",
                "transpile test a has expected js output",
            ): ("transpile", "all"),
            (f"{UNIT} t", "t"): ("unittests", "tsserver"),
        }
        for (full, title), want in cases.items():
            t = pl.Test(full, title)
            self.assertEqual((t.runner, t.bucket), want, full)

    def test_a_title_that_is_not_a_suffix_is_refused(self):
        with self.assertRaises(ValueError):
            pl.Test("a b", "c")

    def test_digest_scheme_equals_the_oracle_digest(self):
        ids = ["b :: x\ny", "a\\b :: z", "a :: \r"]
        self.assertEqual(store.sha256_lines(ids), pl.suite_oracle().sha256_lines(ids))


class SelectionTest(unittest.TestCase):
    def setUp(self):
        self.tests = pl.tests_from_titles(
            compiler_entries("A.ts", "a2.ts", "B.ts", "c.ts", "Zed.ts")
        )
        self.universe = {"compiler": self.tests}

    def names(self, chosen):
        return [p.rsplit("/", 1)[-1] for p, _ in chosen["compiler"]]

    def test_default_selects_everything_by_suite(self):
        chosen = pl.select(self.universe, [])
        self.assertEqual(len(chosen["compiler"]), 5)
        self.assertEqual({len(ts) for _, ts in chosen["compiler"]}, {2})

    def test_bucket_filter(self):
        self.assertEqual(
            self.names(pl.select(self.universe, ["compiler:A,B"])),
            ["A.ts", "a2.ts", "B.ts"],
        )
        self.assertEqual(
            self.names(pl.select(self.universe, ["compiler:Z", "compiler:C"])),
            ["c.ts", "Zed.ts"],
        )

    def test_sample_is_seeded_and_never_empty(self):
        a = pl.select(self.universe, ["compiler"], 0.4, 7)
        b = pl.select(self.universe, ["compiler"], 0.4, 7)
        self.assertEqual(self.names(a), self.names(b))
        self.assertEqual(len(a["compiler"]), 2)
        self.assertEqual(
            len(pl.select(self.universe, ["compiler"], 0.0001, 1)["compiler"]), 1
        )

    def test_a_runner_that_could_not_be_listed_selects_nothing(self):
        self.assertEqual(pl.select({"compiler": None}, ["compiler"]), {"compiler": []})

    def test_unknown_runner(self):
        with self.assertRaises(ValueError):
            pl.parse_spec("nope:A")

    def test_batches_never_split_a_suite(self):
        suites = pl.suites_of(self.tests)
        self.assertEqual(
            [[len(ts) for _, ts in b] for b in pl.batches(suites, 4)],
            [[2, 2], [2, 2], [2]],
        )
        self.assertEqual(
            [[len(ts) for _, ts in b] for b in pl.batches(suites, 1)], [[2]] * 5
        )
        self.assertEqual(pl.batches([], 4), [])

    def test_grep_selects_exactly_the_tests_of_a_batch(self):
        tricky = pl.tests_from_titles(
            [
                ("s (x) [y] a.b+c Correct t", "Correct t"),
                ("s (x) [y] aXb+c Correct t", "Correct t"),
            ]
        )
        batch = [(tricky[0].parent, [tricky[0]])]
        grep = pl.grep_for(batch).replace("\\/", "/")
        hits = [t.full for t in tricky if re.search(grep, t.full)]
        self.assertEqual(hits, [tricky[0].full])


class ScriptedTest(unittest.TestCase):
    def test_parse_results_of_each_oracle_format(self):
        text = (
            "Building...\nok   proj-a\nFAIL proj-b\nPASS watch-c (jsse 1.0s)\n"
            "FAIL watch-d (jsse 2.0s)\nok   hereby-e\ncase  node s  jsse s  result\n"
        )
        self.assertEqual(
            scripted.parse_results(text),
            {
                "proj-a": True,
                "proj-b": False,
                "watch-c": True,
                "watch-d": False,
                "hereby-e": True,
            },
        )

    def test_cases_have_unique_suites_and_are_judged_by_name(self):
        a, b = scripted.Case("hereby", "x"), scripted.Case("hereby", "y")
        self.assertEqual((a.id, a.bucket, a.parent), ("hereby :: x", "all", "x"))
        chosen = pl.select({"hereby": [a, b]}, ["hereby"], 0.5, 1)
        self.assertEqual(len(chosen["hereby"]), 1)
        table = pl.outcome_table({("x", "passing"): 1, ("y", "failing"): 1})
        res = pl.compare([a, b], table, lambda t: True)
        self.assertEqual(
            ([t.id for t in res["pass"]], [t.id for t in res["regression"]]),
            (["hereby :: x"], ["hereby :: y"]),
        )
        self.assertEqual(pl.parse_spec("tsc-watch"), ("tsc-watch", None))

    def test_a_run_that_prints_only_some_cases_is_an_error(self):
        import tempfile

        with tempfile.TemporaryDirectory() as d:
            with open(os.path.join(d, "run-hereby-oracle.py"), "w") as f:
                f.write("print('ok   a')\nprint('FAIL b')\n")
            old = scripted.SCRIPTS
            scripted.SCRIPTS = d
            try:
                cases = [scripted.Case("hereby", n) for n in "abc"]
                out, error = scripted.run_cases(
                    "hereby", cases, sys.executable, "jsse", 1, 60, {}
                )
            finally:
                scripted.SCRIPTS = old
        self.assertEqual(out, {("a", "passing"): 1, ("b", "failing"): 1})
        self.assertIn("1 of 3 cases printed no result", error)

    def test_availability(self):
        self.assertIsNone(scripted.unavailable("hereby", {}))
        self.assertIn("TSC_DIR is not set", scripted.unavailable("tsc-watch", {}))
        self.assertIn(
            "not a typescript 6.0.3 package",
            scripted.unavailable("tsc-projects", {"TSC_DIR": "/nonexistent"}),
        )


class GrepTest(unittest.TestCase):
    def test_line_terminators_are_escaped_because_mocha_cuts_a_grep_at_them(self):
        test = pl.Test("p a\nb\r\u2028c Correct t", "Correct t")
        grep = pl.grep_for([(test.parent, [test])])
        for raw in ("\n", "\r", "\u2028", "\u2029"):
            self.assertNotIn(raw, grep)
        self.assertRegex(test.full, grep)


class CompareTest(unittest.TestCase):
    def test_every_outcome(self):
        tests = pl.tests_from_titles(
            compiler_entries("a.ts", "b.ts", "c.ts", "d.ts", "e.ts")
        )
        pick = lambda n, k: next(t for t in tests if f"/{n}" in t.full and k in t.full)  # noqa: E731
        outcomes = collections.Counter()
        for t in tests:
            outcomes[(t.full, "passing")] += 1
        outcomes[(pick("b.ts", "errors").full, "passing")] = 0
        outcomes[(pick("b.ts", "errors").full, "failing")] = 1
        outcomes[(pick("c.ts", "JS").full, "passing")] = 0
        outcomes[(pick("d.ts", "errors").full, "pending")] = 1
        outcomes[(pick("d.ts", "errors").full, "passing")] = 0
        outcomes[(pick("e.ts", "errors").full, "failing")] = 1
        table = pl.outcome_table({k: v for k, v in outcomes.items() if v})
        held = {
            t.id
            for t in tests
            if "/a.ts" in t.full
            or "/b.ts" in t.full
            or "/c.ts" in t.full
            or "/d.ts" in t.full
        }
        res = pl.compare(tests, table, lambda t: t.id in held)
        self.assertEqual(res["held"], 8)
        self.assertEqual(
            sorted(by_id(res["regression"])),
            sorted(
                [
                    pick("b.ts", "errors").id,
                    pick("c.ts", "JS").id,
                    pick("d.ts", "errors").id,
                ]
            ),
        )
        self.assertEqual(by_id(res["fail"]), [pick("e.ts", "errors").id])
        self.assertEqual(by_id(res["new_pass"]), [pick("e.ts", "JS").id])
        self.assertEqual(len(res["pass"]), 10 - 4)

    def test_duplicate_titles_need_every_occurrence(self):
        twice = pl.tests_from_titles([("p t", "t"), ("p t", "t")])
        ok = pl.outcome_table({("p t", "passing"): 2})
        half = pl.outcome_table({("p t", "passing"): 1})
        self.assertEqual(len(pl.compare(twice, ok, lambda t: True)["pass"]), 2)
        self.assertEqual(len(pl.compare(twice, half, lambda t: True)["regression"]), 2)


def by_id(tests):
    return [t.id for t in tests]


def baseline(*buckets):
    doc = {
        "schema": 1,
        "typescript": {"tag": "v6.0.3", "commit": "c"},
        "jsse": "abc",
        "buckets": {},
    }
    for runner, name, tests in buckets:
        doc["buckets"].setdefault(runner, {})[name] = [tests, "a" * 32]
    return doc


class StoreTest(unittest.TestCase):
    def test_baseline_round_trip(self):
        doc = baseline(("compiler", "A", 3), ("fourslash", "C", 9))
        self.assertEqual(store.parse_baseline(store.dump_baseline(doc)), doc)

    def test_dump_is_stable_one_line_per_bucket(self):
        doc = baseline(
            ("fourslash", "C", 9), ("compiler", "B", 5), ("compiler", "A", 3)
        )
        text = store.dump_baseline(doc)
        self.assertEqual(text.count("\n"), 14)
        self.assertLess(text.index('"A"'), text.index('"B"'))
        self.assertEqual(store.dump_baseline(store.parse_baseline(text)), text)
        self.assertEqual(
            store.parse_baseline(store.dump_baseline(baseline())), baseline()
        )

    def test_malformed_baselines_are_refused(self):
        good = baseline(("compiler", "A", 3))
        bad = [
            "{",
            json.dumps({**good, "schema": 2}),
            json.dumps({**good, "typescript": {"tag": "v"}}),
            json.dumps({**good, "buckets": []}),
            json.dumps({**good, "buckets": {"compiler": {"A": {"tests": 1}}}}),
            json.dumps({**good, "buckets": {"compiler": {"A": [1, "short"]}}}),
            json.dumps({**good, "jsse": 1}),
            json.dumps(baseline(("compiler", "A", True))),
            json.dumps(baseline(("compiler", "A", -1))),
        ]
        for text in bad:
            with self.assertRaises(ValueError, msg=text):
                store.parse_baseline(text)

    def test_not_passing_round_trip_with_awkward_ids(self):
        entries = {
            ("compiler", "A", "a :: x\ty\nz\\w"): "#1418",
            ("conformance", "types", "b :: y"): "#1",
        }
        text = store.dump_not_passing(entries)
        self.assertEqual(store.parse_not_passing(text), entries)
        self.assertEqual(
            len([line for line in text.splitlines() if not line.startswith("#")]), 2
        )

    def test_not_passing_ids_with_unicode_line_separators_round_trip(self):
        entries = {("compiler", "A", "a\u2028b\u0085c\x0bd\x1ce :: t"): "#1"}
        self.assertEqual(
            store.parse_not_passing(store.dump_not_passing(entries)), entries
        )

    def test_malformed_not_passing_lines(self):
        for text in ("a\tb\tc\n", "a\tb\tc\t\n", "a\tb\tc\td\na\tb\tc\te\n"):
            with self.assertRaises(ValueError):
                store.parse_not_passing(text)


class RatchetTest(unittest.TestCase):
    def check(self, base, head):
        return store.ratchet_problems(base, head)

    def test_new_file_and_unchanged_are_fine(self):
        a = (baseline(("compiler", "A", 3)), {})
        self.assertEqual(self.check(None, a), [])
        self.assertEqual(self.check(a, a), [])
        self.assertEqual(self.check(None, None), [])

    def test_gaining_buckets_is_fine(self):
        base = (baseline(("compiler", "A", 3)), {})
        head = (baseline(("compiler", "A", 3), ("compiler", "B", 5)), {})
        self.assertEqual(self.check(base, head), [])

    def test_losing_or_changing_a_bucket_is_a_problem(self):
        base = (baseline(("compiler", "A", 3), ("fourslash", "C", 9)), {})
        removed = (baseline(("compiler", "A", 3)), {})
        changed = (baseline(("compiler", "A", 2), ("fourslash", "C", 9)), {})
        self.assertIn("fourslash:C was removed", self.check(base, removed)[0])
        self.assertIn("compiler:A changed", self.check(base, changed)[0])
        self.assertEqual(len(self.check(base, None)), 1)

    def test_not_passing_may_only_shrink(self):
        key = ("compiler", "A", "x :: y")
        base = (baseline(("compiler", "A", 3)), {key: "#1"})
        gone = (baseline(("compiler", "A", 3)), {})
        more = (
            baseline(("compiler", "A", 3)),
            {key: "#1", ("compiler", "A", "z :: w"): "#2"},
        )
        self.assertEqual(self.check(base, gone), [])
        self.assertIn("gained a not-passing entry", self.check(base, more)[0])

    def test_a_new_bucket_may_bring_its_known_failures(self):
        base = (baseline(("compiler", "A", 3)), {})
        head = (
            baseline(("compiler", "A", 3), ("compiler", "B", 5)),
            {("compiler", "B", "x :: y"): "#1418"},
        )
        self.assertEqual(self.check(base, head), [])

    def test_not_passing_needs_a_recorded_bucket(self):
        head = (baseline(("compiler", "A", 3)), {("compiler", "B", "x :: y"): "#1"})
        self.assertIn("unrecorded bucket compiler:B", self.check(None, head)[0])

    def test_pin_change_needs_a_ruling(self):
        base = (baseline(("compiler", "A", 3)), {})
        head = (json.loads(json.dumps(base[0])), {})
        head[0]["typescript"]["commit"] = "other"
        self.assertIn("pin changed", self.check(base, head)[0])


if __name__ == "__main__":
    unittest.main()
