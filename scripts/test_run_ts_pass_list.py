import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest

SCRIPTS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, SCRIPTS)
import ts_passlist as pl  # noqa: E402
import ts_passlist_store as store  # noqa: E402

GATE = os.path.join(SCRIPTS, "run-ts-pass-list.py")
NODE = shutil.which("node")
ALWAYS = "unittests:: tsbuildWatch:: watchMode:: publicAPI:: Public API with custom transformers"

FAKE_MOCHA = """
const fs = require('fs');
const dry = process.argv.includes('--dry-run');
const runner = JSON.parse(fs.readFileSync('test.config', 'utf8')).runners[0];
const grep = JSON.parse(fs.readFileSync('grep.json', 'utf8')).grep;
const all = JSON.parse(fs.readFileSync('built/local/fake-tests.json', 'utf8'));
const re = grep ? new RegExp(grep) : null;
const tests = (all[runner] || []).filter((t) => !re || re.test(t.fullTitle));
const rep = { tests: [], passes: [], failures: [], pending: [] };
if (!dry && tests.some((t) => t.outcome === 'crash')) process.exit(7);
for (const t of tests) {
  const e = { fullTitle: t.fullTitle, title: t.title };
  rep.tests.push(e);
  if (dry) continue;
  if (t.outcome === 'pass') rep.passes.push(e);
  else if (t.outcome === 'fail') rep.failures.push({ ...e, err: { message: 'boom' } });
}
fs.writeFileSync('report.json', JSON.stringify(rep));
"""


def compiler_tests(files, outcome="pass"):
    out = []
    for f in files:
        for kind in ("errors", "JS output"):
            title = f"Correct {kind} for tests/cases/compiler/{f}"
            full = f"compiler tests compiler tests for tests/cases/compiler/{f} {title}"
            out.append({"fullTitle": full, "title": title, "outcome": outcome})
    return out


def unit_tests(suite, names):
    out = []
    for n in names:
        full = f"unittests:: {suite}:: {n}"
        out.append({"fullTitle": full, "title": n, "outcome": "pass"})
    return out


@unittest.skipUnless(NODE, "node is not installed")
class GateTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        root = self.tmp.name
        self.ts = os.path.join(root, "ts")
        self.data = os.path.join(root, "data")
        os.makedirs(os.path.join(self.ts, "node_modules/mocha/bin"))
        os.makedirs(os.path.join(self.ts, "built/local"))
        os.makedirs(os.path.join(self.ts, "tests/cases/compiler"))
        os.makedirs(self.data)
        self.jsse = os.path.join(root, "fake-jsse")
        with open(self.jsse, "w") as f:
            f.write('#!/bin/sh\nshift\nexec "%s" "$@"\n' % NODE)
        os.chmod(self.jsse, 0o755)
        with open(os.path.join(self.ts, "package.json"), "w") as f:
            json.dump({"version": "6.0.3"}, f)
        with open(os.path.join(self.ts, "built/local/run.js"), "w") as f:
            f.write("")
        with open(os.path.join(self.ts, "node_modules/mocha/bin/_mocha"), "w") as f:
            f.write(FAKE_MOCHA)
        self.compiler = compiler_tests(["a.ts", "b.ts", "c.ts"]) + compiler_tests(
            ["Q.ts"]
        )
        self.units = unit_tests("config", ["x", "y"]) + unit_tests("sys", ["z"])
        self.write_tests()

    def write_tests(self):
        always = {"fullTitle": ALWAYS, "title": ALWAYS, "outcome": "pass"}
        tests = {
            "compiler": [always, *self.compiler],
            "unittest": [always, *self.units],
        }
        with open(os.path.join(self.ts, "built/local/fake-tests.json"), "w") as f:
            json.dump(tests, f)
        runners = {}
        for runner in pl.RUNNERS:
            key = pl.MOCHA_RUNNER.get(runner, runner)
            mine = [
                pl.Test(t["fullTitle"], t["title"])
                for t in tests.get(key, [])
                if pl.Test(t["fullTitle"], t["title"]).runner == runner
            ]
            runners[runner] = {"passed": len(mine), "digest": pl.digest_of(mine)}
        with open(os.path.join(self.tmp.name, "oracle.json"), "w") as f:
            json.dump({"runners": runners}, f)

    def gate(self, *args, env=None, engine="jsse"):
        cmd = [
            sys.executable,
            GATE,
            "--engine",
            engine,
            "--jsse",
            self.jsse,
            "--require",
            "--jobs",
            "2",
            "--batch-size",
            "4",
            "--data-dir",
            self.data,
            "--oracle-json",
            os.path.join(self.tmp.name, "oracle.json"),
            *args,
        ]
        e = {**os.environ, "TS_SRC_DIR": self.ts, **(env or {})}
        r = subprocess.run(cmd, capture_output=True, text=True, env=e)
        return r.returncode, r.stdout + r.stderr

    def summary(self, text):
        line = next(x for x in text.splitlines() if x.startswith("JSON: "))
        return json.loads(line[6:])

    def read(self, name):
        with open(os.path.join(self.data, name), encoding="utf-8") as f:
            return f.read()

    def baseline(self):
        with open(os.path.join(self.data, os.path.basename(store.BASELINE))) as f:
            return store.parse_baseline(f.read())

    def not_passing(self):
        with open(os.path.join(self.data, os.path.basename(store.NOT_PASSING))) as f:
            return store.parse_not_passing(f.read())

    def record(self, *extra):
        code, out = self.gate("compiler", "unittests", "--update-baseline", *extra)
        self.assertEqual(code, 0, out)
        return out

    def test_skips_without_a_checkout(self):
        r = subprocess.run(
            [sys.executable, GATE],
            capture_output=True,
            text=True,
            env={k: v for k, v in os.environ.items() if k != "TS_SRC_DIR"},
        )
        self.assertEqual(
            (r.returncode, r.stdout.startswith("SKIP")), (0, True), r.stdout
        )
        r = subprocess.run(
            [sys.executable, GATE, "--require"],
            capture_output=True,
            text=True,
            env={k: v for k, v in os.environ.items() if k != "TS_SRC_DIR"},
        )
        self.assertEqual(r.returncode, 3)

    def test_only_a_jsse_run_may_update_the_baseline(self):
        code, out = self.gate("compiler", "--update-baseline", engine="node")
        self.assertEqual(code, 2, out)
        self.assertIn("--engine jsse", out)
        self.assertFalse(os.path.exists(os.path.join(self.data, "jsse-pass-1296.json")))

    def test_scripted_oracles_have_no_node_only_run(self):
        code, out = self.gate("hereby", engine="node")
        self.assertEqual(code, 3, out)
        self.assertIn("no Node-only run", out)

    def test_first_run_has_only_new_passes_then_records_buckets(self):
        code, out = self.gate("compiler", "unittests")
        self.assertEqual(code, 0, out)
        s = self.summary(out)
        self.assertEqual(
            (s["baseline"], s["pass"], s["new_pass"], s["regression"]), (0, 12, 12, 0)
        )
        self.record()
        b = self.baseline()["buckets"]
        self.assertEqual(sorted(b["compiler"]), ["A", "B", "C", "Q"])
        self.assertEqual(sorted(b["unittests"]), ["config", "sys", "tsbuildWatch"])
        self.assertEqual(b["compiler"]["A"][0], 2)
        self.assertEqual(self.not_passing(), {})

    def test_a_clean_rerun_has_no_new_passes_and_a_regression_fails(self):
        self.record()
        code, out = self.gate("compiler", "unittests")
        s = self.summary(out)
        self.assertEqual(
            (code, s["baseline"], s["pass"], s["new_pass"]), (0, 12, 12, 0), out
        )
        self.compiler[1]["outcome"] = "fail"
        self.write_tests()
        code, out = self.gate("compiler", "unittests")
        self.assertEqual(code, 1, out)
        self.assertEqual(self.summary(out)["regression"], 1)
        self.assertIn(
            "REGRESSED: compiler tests compiler tests for tests/cases/compiler/a.ts :: Correct JS output",
            out,
        )

    def test_a_test_that_never_reports_is_a_regression(self):
        self.record()
        self.units[0]["outcome"] = "skip"
        self.write_tests()
        code, out = self.gate("unittests")
        self.assertEqual((code, self.summary(out)["regression"]), (1, 1), out)

    def test_runner_and_bucket_selection_limit_the_run(self):
        self.record()
        self.compiler[1]["outcome"] = "fail"
        self.write_tests()
        code, out = self.gate("compiler:B,C", "unittests:sys")
        self.assertEqual((code, self.summary(out)["pass"]), (0, 5), out)

    def test_failing_tests_of_a_new_bucket_need_an_issue(self):
        self.compiler[0]["outcome"] = "fail"
        self.write_tests()
        out = self.record()
        self.assertIn("compiler:A: not recorded, 1 tests fail", out)
        self.assertNotIn("A", self.baseline()["buckets"]["compiler"])
        self.record("--issue", "#1418")
        self.assertIn("A", self.baseline()["buckets"]["compiler"])
        (key,) = self.not_passing()
        self.assertEqual(key[:2], ("compiler", "A"))
        code, out = self.gate("compiler", "unittests")
        s = self.summary(out)
        self.assertEqual(
            (code, s["fail"], s["regression"], s["baseline"]), (0, 1, 0, 11), out
        )

    def test_a_fixed_known_failure_is_a_new_pass_and_leaves_the_not_passing_file(self):
        self.compiler[0]["outcome"] = "fail"
        self.write_tests()
        self.record("--issue", "#1418")
        self.compiler[0]["outcome"] = "pass"
        self.write_tests()
        code, out = self.gate("compiler", "unittests", "--update-baseline")
        self.assertEqual((code, self.summary(out)["new_pass"]), (0, 1), out)
        self.assertEqual(self.not_passing(), {})

    def test_update_refuses_to_run_over_a_regression(self):
        self.record()
        self.compiler[1]["outcome"] = "fail"
        self.compiler.extend(compiler_tests(["d.ts"]))
        self.write_tests()
        before = self.read("jsse-pass-1296.json")
        code, out = self.gate("compiler", "--update-baseline")
        self.assertEqual(code, 1, out)
        self.assertIn("not updating the baseline", out)
        self.assertEqual(self.read("jsse-pass-1296.json"), before)

    def test_a_crashed_batch_fails_the_run(self):
        self.compiler[3]["outcome"] = "crash"
        self.write_tests()
        code, out = self.gate("compiler", "--batch-size", "2")
        self.assertEqual(code, 1, out)
        self.assertIn("no report", out)

    def test_a_universe_that_differs_from_the_pin_fails(self):
        self.record()
        self.compiler.extend(compiler_tests(["d.ts"]))
        with open(os.path.join(self.ts, "built/local/fake-tests.json"), "w") as f:
            json.dump({"compiler": self.compiler, "unittest": self.units}, f)
        code, out = self.gate("compiler")
        self.assertEqual(code, 1, out)
        self.assertIn("ts-suite/oracle.json pins", out)

    def test_sample_runs_a_fraction_of_the_suites(self):
        code, out = self.gate("compiler", "--sample", "0.5", "--seed", "3")
        self.assertEqual((code, self.summary(out)["pass"]), (0, 4), out)

    def test_list_prints_buckets_and_marks_recorded_ones(self):
        self.record("--batch-size", "2")
        code, out = self.gate("compiler", "--list")
        self.assertEqual(code, 0)
        self.assertIn("compiler: A=2*, B=2*, C=2*, Q=2*", out)


if __name__ == "__main__":
    unittest.main()
