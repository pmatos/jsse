import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

SCRIPTS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, SCRIPTS)
import test_run_ts_pass_list as fake  # noqa: E402
import ts_nightly_1298 as nightly  # noqa: E402
import ts_passlist as pl  # noqa: E402
import ts_passlist_store as store  # noqa: E402

CLI = os.path.join(SCRIPTS, "run-ts-suite-nightly-1298.py")
ROOT = os.path.dirname(SCRIPTS)
WORKFLOWS = os.path.join(ROOT, ".github", "workflows")
NIGHTLY = os.path.join(WORKFLOWS, "nightly-ts-suite-1298.yml")
SMOKE = os.path.join(WORKFLOWS, "ts-suite-nightly-smoke.yml")
NODE = shutil.which("node")


class T:
    def __init__(self, ident):
        self.id = ident


def chosen_of(sizes):
    return {
        runner: [
            (f"{runner}-{i}", [T(f"{runner}-{i}-{j}") for j in range(n)])
            for i, n in enumerate(ns)
        ]
        for runner, ns in sizes.items()
    }


class PartitionTest(unittest.TestCase):
    SIZES = {
        "compiler": [30, 20, 20, 10, 5, 5, 1, 1, 1],
        "fourslash-server": [3, 2, 1],
        "unittests": [40, 7],
    }

    def names(self, part):
        return {r: sorted(p for p, _ in s) for r, s in part.items()}

    def test_each_suite_runs_in_exactly_one_shard(self):
        chosen = chosen_of(self.SIZES)
        parts = [nightly.partition(chosen, i, 5) for i in range(5)]
        for runner, suites in chosen.items():
            got = sorted(p for part in parts for p, _ in part.get(runner, []))
            self.assertEqual(got, sorted(p for p, _ in suites))

    def test_shards_are_balanced_by_estimated_cost(self):
        chosen = chosen_of(self.SIZES)
        cost = [
            sum(
                nightly.WEIGHT[r] * len(ts)
                for r, suites in nightly.partition(chosen, i, 4).items()
                for _, ts in suites
            )
            for i in range(4)
        ]
        self.assertLess(max(cost) - min(cost), max(nightly.WEIGHT.values()) * 3)

    def test_assignment_does_not_depend_on_suite_order(self):
        chosen = chosen_of(self.SIZES)
        flipped = {r: list(reversed(s)) for r, s in chosen.items()}
        for i in range(3):
            self.assertEqual(
                self.names(nightly.partition(chosen, i, 3)),
                self.names(nightly.partition(flipped, i, 3)),
            )

    def test_a_runner_without_a_suite_in_the_shard_is_dropped(self):
        chosen = chosen_of({"hereby": [1, 1], "compiler": [5]})
        parts = [nightly.partition(chosen, i, 3) for i in range(3)]
        self.assertEqual(sum("hereby" in p for p in parts), 2)
        self.assertTrue(all(r for p in parts for r in p.values()))

    def test_every_nightly_runner_has_a_weight(self):
        self.assertTrue(set(nightly.DEFAULT_RUNNERS) <= set(nightly.WEIGHT))
        self.assertIn("tsc-projects", nightly.DEFAULT_RUNNERS)

    def test_a_shard_out_of_range_is_refused(self):
        for shard, shards in ((3, 3), (-1, 3), (0, 0)):
            with self.assertRaises(ValueError):
                nightly.partition({}, shard, shards)

    def test_pass_list_lines_escape_like_the_oracle(self):
        self.assertEqual(nightly.line_of("a\\b\nc\rd"), "a\\\\b\\nc\\rd")


def summary(shard, **kw):
    s = {
        "schema": nightly.SCHEMA,
        "shard": shard,
        "shards": 2,
        "ran": True,
        "exit": 0,
        "secs": 10,
        "universe": {"compiler": 4},
        "selected": {"compiler": 2},
        "runners": {
            "compiler": {
                "held": 2,
                "pass": 2,
                "fail": 0,
                "new_pass": 0,
                "regression": 0,
            }
        },
        "batch_errors": [],
        "integrity": [],
    }
    s.update(kw)
    return s


REGRESSED = {
    "compiler": {"held": 2, "pass": 1, "fail": 0, "new_pass": 0, "regression": 1}
}


class MergeTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = os.path.join(self.tmp.name, "in")
        self.out = os.path.join(self.tmp.name, "out")

    def shard(self, s, passed=(), regressed=()):
        r = "compiler"
        nightly.write_shard(
            os.path.join(self.root, f"ts-suite-shard-{s['shard']}"),
            s,
            {r: list(passed)},
            {r: []},
            {r: list(regressed)},
            [nightly.timing_record(r, f"{r}-0", 2, 1.5, 0)],
        )

    def merge(self, **kw):
        kw.setdefault("runners", ["compiler"])
        return nightly.merge(self.root, self.out, 2, **kw)

    def read(self, name):
        with open(os.path.join(self.out, name), encoding="utf-8") as f:
            return f.read()

    def test_clean_run_merges_pass_lists_and_timings(self):
        self.shard(summary(0), ["b", "a\nx"])
        self.shard(summary(1), ["c", "d"])
        self.assertEqual(self.merge(), (0, []))
        self.assertEqual(self.read("pass-compiler.txt"), "a\\nx\nb\nc\nd\n")
        timings = json.loads(self.read("timings.json"))
        self.assertEqual([t["shard"] for t in timings], [0, 1])
        self.assertEqual(json.loads(self.read("summary.json"))["total"]["pass"], 4)
        self.assertIn("| **total** | 4 | 4 | 0 | 0 | 0 |", self.read("summary.md"))

    def test_a_missing_shard_fails_in_every_mode(self):
        self.shard(summary(0))
        for blocking in (False, True):
            code, messages = self.merge(blocking=blocking)
            self.assertEqual(code, 1)
            self.assertTrue(any("shard 1: no results" in m for m in messages))

    def test_a_suite_run_twice_or_never_fails(self):
        self.shard(summary(0))
        self.shard(summary(1, selected={"compiler": 3}))
        code, messages = self.merge()
        self.assertEqual(code, 1)
        self.assertTrue(
            any("shards ran 5 tests, the universe has 4" in m for m in messages)
        )
        self.assertEqual(self.merge(sampled=True)[0], 0)

    def test_a_universe_that_differs_between_shards_fails(self):
        self.shard(summary(0))
        self.shard(summary(1, universe={"compiler": 5}))
        code, messages = self.merge()
        self.assertEqual(code, 1)
        self.assertTrue(any("different test universes" in m for m in messages))

    def test_a_shard_that_did_not_run_the_gate_fails(self):
        self.shard(summary(0))
        self.shard(summary(1, ran=False, exit=3))
        code, messages = self.merge()
        self.assertEqual(code, 1)
        self.assertTrue(any("did not run (exit 3)" in m for m in messages))

    def test_integrity_problems_of_a_shard_fail_in_every_mode(self):
        self.shard(summary(0))
        self.shard(summary(1, integrity=["compiler: 9 tests listed, pin has 8"]))
        code, messages = self.merge()
        self.assertEqual(code, 1)
        self.assertTrue(any("shard 1: compiler: 9 tests listed" in m for m in messages))

    def test_regressions_warn_unless_the_run_is_blocking(self):
        self.shard(summary(0, runners=REGRESSED), ["a"], regressed=["b"])
        self.shard(summary(1, runners=REGRESSED), ["c"], regressed=["d"])
        code, messages = self.merge()
        self.assertEqual(code, 0)
        self.assertEqual(messages, ["warning: 2 baseline regression(s)"])
        self.assertEqual(self.read("regression-compiler.txt"), "b\nd\n")
        code, messages = self.merge(blocking=True)
        self.assertEqual(code, 1)
        self.assertEqual(messages, ["error: 2 baseline regression(s)"])

    def test_a_crashed_batch_is_a_gating_problem(self):
        self.shard(summary(0, batch_errors=["compiler-3: exit 137, no report"]))
        self.shard(summary(1))
        self.assertEqual(
            self.merge(),
            (0, ["warning: shard 0: compiler-3: exit 137, no report"]),
        )
        self.assertEqual(self.merge(blocking=True)[0], 1)

    def test_a_runner_missing_from_the_universe_fails(self):
        self.shard(summary(0))
        self.shard(summary(1))
        code, messages = self.merge(runners=["compiler", "hereby"])
        self.assertEqual(code, 1)
        self.assertTrue(
            any(m.startswith("error: hereby: not in the universe") for m in messages)
        )


class WorkflowTest(unittest.TestCase):
    def text(self, path):
        with open(path, encoding="utf-8") as f:
            return f.read()

    def test_matrix_lists_every_shard(self):
        text = self.text(NIGHTLY)
        shards = int(re.search(r'TS_SHARDS: "(\d+)"', text).group(1))
        matrix = re.search(r"shard: \[([0-9, ]+)\]", text).group(1)
        self.assertEqual([int(x) for x in matrix.split(",")], list(range(shards)))
        self.assertIn(f"--shards {shards}", text.replace('"$TS_SHARDS"', str(shards)))

    def test_every_action_is_pinned_by_sha_and_npx_is_not_used(self):
        for path in (NIGHTLY, SMOKE):
            text = self.text(path)
            uses = re.findall(r"uses: (\S+)", text)
            self.assertTrue(uses)
            for u in uses:
                self.assertRegex(u, r"@[0-9a-f]{40}$")
            self.assertNotIn("npx", text)

    def test_the_blocking_switch_is_one_documented_line_before_the_jobs(self):
        text = self.text(NIGHTLY)
        self.assertEqual(len(re.findall(r'^\s*TS_SUITE_BLOCKING: "', text, re.M)), 1)
        head = text.split("\njobs:")[0]
        self.assertRegex(head, r'TS_SUITE_BLOCKING: "true"')
        self.assertIn("blocking", head.split("TS_SUITE_BLOCKING")[0].lower())


@unittest.skipUnless(NODE, "node is not installed")
class ShardEndToEndTest(unittest.TestCase):
    """Two shards of the real gate over a fake mocha, then the merge."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        t = self.tmp.name
        self.ts = os.path.join(t, "ts")
        self.data = os.path.join(t, "data")
        for d in ("node_modules/mocha/bin", "built/local", "tests/cases/compiler"):
            os.makedirs(os.path.join(self.ts, d))
        os.makedirs(self.data)
        with open(os.path.join(self.ts, "package.json"), "w") as f:
            json.dump({"version": "6.0.3"}, f)
        for rel, text in (
            ("built/local/run.js", ""),
            ("node_modules/mocha/bin/_mocha", fake.FAKE_MOCHA),
        ):
            with open(os.path.join(self.ts, rel), "w") as f:
                f.write(text)
        self.jsse = os.path.join(t, "fake-jsse")
        with open(self.jsse, "w") as f:
            f.write('#!/bin/sh\nshift\nexec "%s" "$@"\n' % NODE)
        os.chmod(self.jsse, 0o755)
        self.compiler = fake.compiler_tests(["a.ts", "b.ts", "c.ts", "Q.ts"])
        self.units = fake.unit_tests("config", ["x", "y"]) + fake.unit_tests(
            "sys", ["z"]
        )
        self.write_tests()

    def write_tests(self):
        tests = {"compiler": self.compiler, "unittest": self.units}
        with open(os.path.join(self.ts, "built/local/fake-tests.json"), "w") as f:
            json.dump(tests, f)
        runners, buckets = {}, {}
        for runner in pl.RUNNERS:
            mine = [
                pl.Test(x["fullTitle"], x["title"])
                for x in tests.get(pl.MOCHA_RUNNER.get(runner, runner), [])
            ]
            runners[runner] = {"passed": len(mine), "digest": pl.digest_of(mine)}
            for b in {x.bucket for x in mine}:
                ids = [x.id for x in mine if x.bucket == b]
                buckets.setdefault(runner, {})[b] = [len(ids), store.bucket_digest(ids)]
        with open(os.path.join(self.tmp.name, "oracle.json"), "w") as f:
            json.dump({"runners": runners}, f)
        with open(os.path.join(ROOT, "ts-suite", "pin.json"), encoding="utf-8") as f:
            doc = store.empty_baseline(json.load(f))
        doc["buckets"] = buckets
        with open(os.path.join(self.data, "jsse-pass-1296.json"), "w") as f:
            f.write(store.dump_baseline(doc))

    def run_shards(self, n=2):
        env = {**os.environ, "TS_SRC_DIR": self.ts}
        out = os.path.join(self.tmp.name, "results")
        for i in range(n):
            cmd = [sys.executable, CLI, "shard", "--shard", str(i), "--shards", str(n)]
            cmd += ["--out", os.path.join(out, f"s{i}"), "--jsse", self.jsse]
            cmd += ["--jobs", "2", "--batch-size", "4", "--data-dir", self.data]
            cmd += ["--oracle-json", os.path.join(self.tmp.name, "oracle.json")]
            r = subprocess.run(
                cmd + ["compiler", "unittests"], capture_output=True, text=True, env=env
            )
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        return out

    def merge(self, results, *extra):
        cmd = [sys.executable, CLI, "merge", "--results", results]
        cmd += ["--out", os.path.join(self.tmp.name, "merged"), "--shards", "2"]
        cmd += ["--runners", "compiler,unittests", *extra]
        r = subprocess.run(cmd, capture_output=True, text=True)
        return r.returncode, r.stdout

    def merged(self, name):
        with open(os.path.join(self.tmp.name, "merged", name), encoding="utf-8") as f:
            return f.read().splitlines()

    def test_shards_cover_the_universe_once_and_merge_clean(self):
        code, out = self.merge(self.run_shards())
        self.assertEqual(code, 0, out)
        self.assertEqual(len(self.merged("pass-compiler.txt")), 8)
        self.assertEqual(len(self.merged("pass-unittests.txt")), 3)
        with open(os.path.join(self.tmp.name, "merged", "timings.json")) as f:
            timings = json.load(f)
        self.assertTrue(
            any(t["runner"] == "compiler" and not t["dry"] for t in timings)
        )
        self.assertTrue(any(t["runner"] == "compiler" and t["dry"] for t in timings))

    def test_a_regression_is_reported_and_blocks_only_on_request(self):
        self.compiler[1]["outcome"] = "fail"
        self.write_tests()
        results = self.run_shards()
        code, out = self.merge(results)
        self.assertEqual(code, 0, out)
        self.assertIn("warning: 1 baseline regression(s)", out)
        self.assertEqual(len(self.merged("regression-compiler.txt")), 1)
        self.assertEqual(self.merge(results, "--blocking")[0], 1)

    def test_a_missing_shard_directory_fails_the_merge(self):
        results = self.run_shards()
        shutil.rmtree(os.path.join(results, "s1"))
        self.assertEqual(self.merge(results)[0], 1)


if __name__ == "__main__":
    unittest.main()
