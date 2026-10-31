import os
import subprocess
import sys
import tempfile
import types
import unittest

SCRIPTS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, SCRIPTS)
import ts_passlist_cases_1298 as cases  # noqa: E402
import ts_passlist_scripted as scripted  # noqa: E402
import ts_passlist_store as store  # noqa: E402

CHECK = os.path.join(SCRIPTS, "check_ts_pass_list_ratchet.py")
ENV = {
    **os.environ,
    "GIT_AUTHOR_NAME": "t",
    "GIT_AUTHOR_EMAIL": "t@example.com",
    "GIT_COMMITTER_NAME": "t",
    "GIT_COMMITTER_EMAIL": "t@example.com",
}
R = "tsc-projects"


def doc(**buckets):
    out = {
        "schema": 1,
        "typescript": {"tag": "v6.0.3", "commit": "c"},
        "jsse": "abc",
        "buckets": {},
    }
    for runner, entry in buckets.items():
        out["buckets"][runner.replace("_", "-")] = {"all": entry}
    return out


def entry(names, runner=R):
    return cases.entry_of(runner, names)


def side(names=None, *, digest_only=None, added=(), mocha=None, np=None):
    """A ratchet side: the scripted bucket with its case list, or recorded as a digest only."""
    d = doc()
    lists = {}
    if names is not None:
        d["buckets"][R] = {"all": entry(names)}
        lists = {R: {"all": {"cases": list(names), "added": list(added)}}}
    elif digest_only is not None:
        d["buckets"][R] = {"all": entry(digest_only)}
    if mocha:
        d["buckets"]["compiler"] = {"A": mocha}
    return d, np or {}, lists


def check(base, head):
    grown, extra = cases.growth(base, head)
    return store.ratchet_problems(base and base[:2], head and head[:2], grown) + extra


class RatchetRulesTest(unittest.TestCase):
    def test_a_scripted_bucket_may_grow(self):
        self.assertEqual(check(side("abc"), side("abcd")), [])
        self.assertEqual(check(side("ab"), side("ab")), [])

    def test_a_shrink_fails(self):
        problems = check(side("abc"), side("ab"))
        self.assertTrue(any("lost 1 recorded case" in p for p in problems), problems)

    def test_a_rename_fails_even_when_the_bucket_grows(self):
        self.assertTrue(check(side("abc"), side("abxd")))
        self.assertTrue(check(side("abc"), side("abx")))

    def test_the_case_list_must_hash_to_the_recorded_bucket(self):
        head = side("abcd")
        head[0]["buckets"][R]["all"] = entry("abc")
        self.assertTrue(any("does not hash" in p for p in check(side("abc"), head)))

    def test_the_case_list_cannot_be_dropped_or_the_bucket_changed_without_it(self):
        self.assertTrue(
            any("removed" in p for p in check(side("abc"), side(digest_only="abc")))
        )
        self.assertTrue(
            any(
                "changed" in p
                for p in check(side(digest_only="abc"), side(digest_only="abcd"))
            )
        )

    def test_a_digest_only_record_grows_when_added_proves_containment(self):
        self.assertEqual(check(side(digest_only="abc"), side("abcde", added="de")), [])
        self.assertEqual(check(side(digest_only="abc"), side("abc")), [])

    def test_a_digest_only_record_does_not_grow_with_a_false_added(self):
        self.assertTrue(check(side(digest_only="abc"), side("abcde")))
        self.assertTrue(check(side(digest_only="abc"), side("abcde", added="cde")))
        self.assertTrue(check(side(digest_only="abc"), side("abxde", added="de")))

    def test_added_is_not_consulted_once_the_base_has_names(self):
        self.assertEqual(check(side("abcde", added="de"), side("abcdef", added="")), [])

    def test_a_mocha_bucket_never_grows(self):
        base = side(mocha=[3, "a" * 32])
        self.assertTrue(
            any("changed" in p for p in check(base, side(mocha=[4, "b" * 32])))
        )
        self.assertEqual(check(base, side(mocha=[3, "a" * 32])), [])

    def test_a_grown_bucket_still_cannot_gain_a_known_failure(self):
        head = side("abcd", np={(R, "all", "tsc-projects :: d"): "#1"})
        self.assertTrue(any("not-passing" in p for p in check(side("abc"), head)))

    def test_a_case_list_of_a_mocha_runner_is_malformed(self):
        text = '{"schema": 1, "buckets": {"compiler": {"A": {"cases": ["x"]}}}}'
        with self.assertRaises(ValueError):
            cases.parse_cases(text)

    def test_parse_rejects_duplicates_and_a_foreign_added(self):
        for body in (
            '{"cases": ["a", "a"]}',
            '{"cases": ["a"], "added": ["b"]}',
            '{"added": []}',
        ):
            with self.assertRaises(ValueError, msg=body):
                cases.parse_cases(
                    '{"schema": 1, "buckets": {"%s": {"all": %s}}}' % (R, body)
                )

    def test_dump_round_trips_and_sorts(self):
        data = {R: {"all": {"cases": ["b", "a"], "added": ["b"]}}}
        text = cases.dump_cases(data)
        self.assertEqual(
            cases.parse_cases(text), {R: {"all": {"cases": ["a", "b"], "added": ["b"]}}}
        )
        self.assertEqual(text, cases.dump_cases(cases.parse_cases(text)))


class HookTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.repo = self.tmp.name
        self.addCleanup(self.tmp.cleanup)
        self.git("init", "-q", "-b", "main")
        os.makedirs(os.path.join(self.repo, "ts-suite"))

    def git(self, *args):
        return subprocess.run(
            ["git", *args],
            cwd=self.repo,
            env=ENV,
            check=True,
            capture_output=True,
            text=True,
        ).stdout

    def write(self, names, digest_only=None, added=()):
        d, np, lists = side(names, digest_only=digest_only, added=added)
        for rel, text in (
            (store.BASELINE, store.dump_baseline(d)),
            (store.NOT_PASSING, store.dump_not_passing(np)),
        ):
            with open(os.path.join(self.repo, rel), "w") as f:
                f.write(text)
        path = os.path.join(self.repo, cases.CASES)
        if lists:
            with open(path, "w") as f:
                f.write(cases.dump_cases(lists))
        elif os.path.exists(path):
            os.remove(path)

    def commit(self):
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "c")

    def check(self, *args):
        r = subprocess.run(
            [sys.executable, CHECK, *args],
            cwd=self.repo,
            capture_output=True,
            text=True,
        )
        return r.returncode, r.stderr

    def test_the_hook_lets_a_scripted_bucket_grow_from_a_digest_and_then_from_names(
        self,
    ):
        self.write(None, digest_only="abc")
        self.commit()
        self.write("abcde", added="de")
        self.git("add", "-A")
        self.assertEqual(self.check(), (0, ""))
        self.commit()
        self.write("abcdef", added="de")
        self.git("add", "-A")
        self.assertEqual(self.check(), (0, ""))

    def test_the_hook_and_ci_mode_refuse_a_rename_and_a_shrink(self):
        self.write("abc")
        self.commit()
        base = self.git("rev-parse", "HEAD").strip()
        for names in ("abxd", "ab"):
            self.write(names)
            self.git("add", "-A")
            code, err = self.check()
            self.assertEqual(code, 1, err)
            self.assertIn("recorded case", err)
        self.commit()
        code, err = self.check("--base", base, "--head", "HEAD")
        self.assertEqual(code, 1, err)

    def test_a_malformed_case_list_is_reported(self):
        self.write("abc")
        self.commit()
        with open(os.path.join(self.repo, cases.CASES), "w") as f:
            f.write("{")
        self.git("add", "-A")
        code, err = self.check()
        self.assertEqual(code, 1)
        self.assertIn("malformed", err)


class Case(scripted.Case):
    pass


def tests_of(names, runner=R):
    return [Case(runner, n) for n in names]


class GateTest(unittest.TestCase):
    def universe(self, names):
        return {R: tests_of(names)}

    def test_bucket_problems_accept_growth_and_flag_a_lost_case(self):
        d, _, lists = side("abc")
        known = cases.resolved(d, lists, {})
        self.assertEqual(cases.bucket_problems(d, self.universe("abcd"), known), [])
        problems = cases.bucket_problems(d, self.universe("abx"), known)
        self.assertEqual(len(problems), 1)
        self.assertIn("lacks", problems[0])

    def test_a_digest_only_bucket_keeps_the_exact_check_without_legacy_names(self):
        d, _, lists = side(digest_only="abc")
        self.assertEqual(
            cases.bucket_problems(
                d, self.universe("abc"), cases.resolved(d, lists, {})
            ),
            [],
        )
        problems = cases.bucket_problems(
            d, self.universe("abcd"), cases.resolved(d, lists, {})
        )
        self.assertIn("has 3 tests, the checkout lists 4", problems[0])

    def test_legacy_names_count_only_when_they_hash_to_the_record(self):
        d, _, lists = side(digest_only="abc")
        self.assertEqual(
            cases.resolved(d, lists, {R: list("abc")}), {(R, "all"): set("abc")}
        )
        self.assertEqual(cases.resolved(d, lists, {R: list("abx")}), {})

    def test_mocha_buckets_keep_the_exact_check(self):
        d = doc()
        d["buckets"]["compiler"] = {"A": [1, "0" * 32]}
        test = types.SimpleNamespace(bucket="A", id="x", full="x", runner="compiler")
        problems = cases.bucket_problems(d, {"compiler": [test]}, {})
        self.assertIn("recorded bucket compiler:A", problems[0])

    def test_held_is_by_case_name_for_a_known_scripted_bucket(self):
        d, _, lists = side("abc")
        is_held = cases.held(d, cases.resolved(d, lists, {}))
        self.assertTrue(is_held(Case(R, "a")))
        self.assertFalse(is_held(Case(R, "z")))
        mocha = types.SimpleNamespace(bucket="A", runner="compiler", full="t")
        d["buckets"]["compiler"] = {"A": [1, "0" * 32]}
        self.assertTrue(cases.held(d, {})(mocha))

    def run_of(self, names, failing=()):
        ts = tests_of(names)
        return ts, {"passing_ids": {t.id for t in ts if t.full not in failing}}

    def test_record_grows_a_digest_only_bucket_with_added(self):
        d, _, lists = side(digest_only="abc")
        tests, run = self.run_of("abcde")
        known = cases.resolved(d, lists, {R: list("abc")})
        new_doc, new_cases, notes = cases.record_scripted(
            d, lists, run, {(R, "all"): tests}, known, "rev"
        )
        self.assertEqual(new_doc["buckets"][R]["all"], entry("abcde"))
        self.assertEqual(
            new_cases[R]["all"], {"cases": list("abcde"), "added": ["d", "e"]}
        )
        self.assertEqual(check((d, {}, lists), (new_doc, {}, new_cases)), [])
        self.assertIn("2 new", notes[0])

    def test_record_needs_the_legacy_names_to_grow_a_digest_only_bucket(self):
        d, _, lists = side(digest_only="abc")
        tests, run = self.run_of("abcde")
        new_doc, new_cases, notes = cases.record_scripted(
            d, lists, run, {(R, "all"): tests}, {}, "rev"
        )
        self.assertEqual((new_doc, new_cases), (d, lists))
        self.assertIn("--legacy-cases", notes[0])

    def test_record_adopts_the_names_of_an_unchanged_digest_only_bucket(self):
        d, _, lists = side(digest_only="abc")
        tests, run = self.run_of("abc")
        new_doc, new_cases, _ = cases.record_scripted(
            d, lists, run, {(R, "all"): tests}, {}, "rev"
        )
        self.assertEqual(new_doc["buckets"], d["buckets"])
        self.assertEqual(new_cases[R]["all"], {"cases": list("abc"), "added": []})

    def test_record_refuses_a_failing_case_and_a_missing_recorded_case(self):
        d, _, lists = side("abc")
        known = cases.resolved(d, lists, {})
        tests, run = self.run_of("abcd", failing={"d"})
        out = cases.record_scripted(d, lists, run, {(R, "all"): tests}, known, "rev")
        self.assertEqual(out[:2], (d, lists))
        self.assertIn("1 case(s) fail", out[2][0])
        tests, run = self.run_of("abx")
        out = cases.record_scripted(d, lists, run, {(R, "all"): tests}, known, "rev")
        self.assertEqual(out[:2], (d, lists))
        self.assertIn("gone", out[2][0])

    def test_recording_twice_changes_nothing_and_keeps_added(self):
        d, _, lists = side(digest_only="abc")
        tests, run = self.run_of("abcde")
        known = cases.resolved(d, lists, {R: list("abc")})
        once = cases.record_scripted(d, lists, run, {(R, "all"): tests}, known, "rev")
        known = cases.resolved(once[0], once[1], {})
        twice = cases.record_scripted(
            once[0], once[1], run, {(R, "all"): tests}, known, "rev2"
        )
        self.assertEqual(twice[:2], once[:2])
        self.assertEqual(twice[2], [])


ROOT = os.path.dirname(SCRIPTS)


class CheckedInFilesTest(unittest.TestCase):
    def load(self):
        with open(os.path.join(ROOT, store.BASELINE), encoding="utf-8") as f:
            doc = store.parse_baseline(f.read())
        with open(os.path.join(ROOT, cases.CASES), encoding="utf-8") as f:
            return doc, cases.parse_cases(f.read())

    def test_the_case_lists_hash_to_the_recorded_buckets(self):
        doc, lists = self.load()
        self.assertEqual(cases.growth(None, (doc, {}, lists)), (set(), []))
        self.assertEqual(len(lists[R]["all"]["cases"]), 413)

    def test_every_recorded_case_is_still_a_fixture_case(self):
        _, lists = self.load()
        r = subprocess.run(
            [
                sys.executable,
                os.path.join(SCRIPTS, "run-tsc-projects-oracle.py"),
                "--list",
            ],
            capture_output=True,
            text=True,
            check=True,
        )
        self.assertLessEqual(set(lists[R]["all"]["cases"]), set(r.stdout.split()))


if __name__ == "__main__":
    unittest.main()
