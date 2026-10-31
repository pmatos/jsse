import json
import os
import subprocess
import sys
import tempfile
import unittest

SCRIPTS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, SCRIPTS)
import ts_passlist_store as store  # noqa: E402

CHECK = os.path.join(SCRIPTS, "check_ts_pass_list_ratchet.py")
ENV = {
    **os.environ,
    "GIT_AUTHOR_NAME": "t",
    "GIT_AUTHOR_EMAIL": "t@example.com",
    "GIT_COMMITTER_NAME": "t",
    "GIT_COMMITTER_EMAIL": "t@example.com",
}


def doc(*buckets):
    out = {
        "schema": 1,
        "typescript": {"tag": "v6.0.3", "commit": "c"},
        "jsse": "abc",
        "buckets": {},
    }
    for runner, name, tests in buckets:
        out["buckets"].setdefault(runner, {})[name] = [tests, "b" * 32]
    return out


class RatchetHookTest(unittest.TestCase):
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

    def write(self, baseline=None, not_passing=None):
        if baseline is not None:
            with open(os.path.join(self.repo, store.BASELINE), "w") as f:
                f.write(
                    baseline
                    if isinstance(baseline, str)
                    else store.dump_baseline(baseline)
                )
        if not_passing is not None:
            with open(os.path.join(self.repo, store.NOT_PASSING), "w") as f:
                f.write(store.dump_not_passing(not_passing))

    def commit(self, message="c"):
        self.git("add", "-A")
        self.git("commit", "-q", "-m", message)

    def check(self, *args):
        r = subprocess.run(
            [sys.executable, CHECK, *args],
            cwd=self.repo,
            capture_output=True,
            text=True,
        )
        return r.returncode, r.stderr

    def test_first_commit_of_the_files_is_fine(self):
        self.write(doc(("compiler", "A", 3)), {})
        self.git("add", "-A")
        self.assertEqual(self.check()[0], 0)

    def test_gaining_a_bucket_passes_and_losing_one_fails(self):
        self.write(doc(("compiler", "A", 3), ("fourslash", "C", 9)), {})
        self.commit()
        self.write(
            doc(("compiler", "A", 3), ("fourslash", "C", 9), ("transpile", "all", 37))
        )
        self.git("add", "-A")
        self.assertEqual(self.check()[0], 0)
        self.write(doc(("compiler", "A", 3)))
        self.git("add", "-A")
        code, err = self.check()
        self.assertEqual(code, 1)
        self.assertIn("fourslash:C was removed", err)

    def test_not_passing_growth_fails_and_shrinking_passes(self):
        key = ("compiler", "A", "x :: y")
        self.write(doc(("compiler", "A", 3)), {key: "#1"})
        self.commit()
        self.write(not_passing={key: "#1", ("compiler", "A", "z :: w"): "#2"})
        self.git("add", "-A")
        code, err = self.check()
        self.assertEqual(code, 1)
        self.assertIn("gained a not-passing entry", err)
        self.write(not_passing={})
        self.git("add", "-A")
        self.assertEqual(self.check()[0], 0)

    def test_only_the_index_counts(self):
        self.write(doc(("compiler", "A", 3)), {})
        self.commit()
        self.write(doc())
        self.assertEqual(self.check()[0], 0)

    def test_deleting_the_baseline_fails(self):
        self.write(doc(("compiler", "A", 3)), {})
        self.commit()
        self.git("rm", "-q", store.BASELINE)
        self.assertEqual(self.check()[0], 1)

    def test_malformed_file_fails(self):
        self.write("{}")
        self.git("add", "-A")
        code, err = self.check()
        self.assertEqual(code, 1)
        self.assertIn("malformed", err)

    def test_ci_mode_compares_the_merge_base_with_head(self):
        self.write(doc(("compiler", "A", 3)), {})
        self.commit("base")
        self.git("checkout", "-q", "-b", "feature")
        self.write(doc(("compiler", "A", 3), ("compiler", "B", 4)))
        self.commit("gain")
        self.assertEqual(self.check("--base", "main", "--head", "feature")[0], 0)
        self.write(doc())
        self.commit("lose")
        code, err = self.check("--base", "main", "--head", "feature")
        self.assertEqual(code, 1)
        self.assertIn("compiler:A was removed", err)
        self.git("checkout", "-q", "main")
        self.assertEqual(self.check("--base", "main")[0], 0)

    def test_head_without_base_is_a_usage_error(self):
        self.assertEqual(self.check("--head", "x")[0], 2)


class CheckedInFilesTest(unittest.TestCase):
    def test_the_checked_in_pass_list_is_well_formed(self):
        root = os.path.dirname(SCRIPTS)
        with open(os.path.join(root, store.BASELINE), encoding="utf-8") as f:
            d = store.parse_baseline(f.read())
        with open(os.path.join(root, store.NOT_PASSING), encoding="utf-8") as f:
            np = store.parse_not_passing(f.read())
        self.assertEqual(store.ratchet_problems(None, (d, np)), [])
        with open(os.path.join(root, "ts-suite", "pin.json"), encoding="utf-8") as f:
            pin = json.load(f)
        self.assertEqual(d["typescript"], {"tag": pin["tag"], "commit": pin["commit"]})
        with open(os.path.join(root, "ts-suite", "oracle.json"), encoding="utf-8") as f:
            runners = json.load(f)["runners"]
        for runner, table in d["buckets"].items():
            if runner in runners:
                self.assertLessEqual(
                    sum(e[0] for e in table.values()), runners[runner]["passed"]
                )
            else:
                self.assertIn(runner, ("tsc-projects", "tsc-watch", "hereby"))


if __name__ == "__main__":
    unittest.main()
