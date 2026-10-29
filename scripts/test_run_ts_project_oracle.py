import importlib.util
import json
import os
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))
import ts_project_control as control  # noqa: E402
import ts_project_slices as ps  # noqa: E402

spec = importlib.util.spec_from_file_location(
    "run_ts_project_oracle", SCRIPTS_DIR / "run-ts-project-oracle.py"
)
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)

ITS = (
    "Correct module resolution tracing",
    "Correct errors",
    "Correct JS output",
    "Correct declarations",
)


def entries(*names):
    out = []
    for n in names:
        file = f"tests/cases/project/{n}"
        for kind in ("commonjs", "amd"):
            key = f"projects tests Compiling project for scenario of {n}: testcase {file} (@module: {kind})"
            out += [(f"{key} {it} for {file}", f"{it} for {file}") for it in ITS]
    return out


NAMES = (
    "baseline.json",
    "outSimple.json",
    "Declarations.json",
    "mapRootA.json",
    "SourcemapB.json",
    "z.json",
)
CASES = ps.cases_from_titles(entries(*NAMES))


class SelectTest(unittest.TestCase):
    def names(self, text, table=None):
        return [c.base for c in ps.select(text, CASES, table or ps.SLICES)]

    def test_a_case_is_one_json_file_with_both_module_kinds(self):
        self.assertEqual([c.base for c in CASES], list(NAMES))
        self.assertEqual(len(CASES[0].tests), 8)
        self.assertEqual(len(CASES[0].describes), 2)
        self.assertEqual(ps.cases_from_titles([("projects tests other", "other")]), [])

    def test_core_and_matrix_partition_the_cases(self):
        core, matrix = self.names("core"), self.names("matrix")
        self.assertEqual(core, ["baseline.json", "Declarations.json", "z.json"])
        self.assertEqual(matrix, ["outSimple.json", "mapRootA.json", "SourcemapB.json"])
        self.assertEqual(sorted(core + matrix), sorted(NAMES))
        self.assertEqual(self.names("all"), list(NAMES))

    def test_startswith_except_and_base_terms(self):
        self.assertEqual(self.names("startswith:decl"), ["Declarations.json"])
        self.assertEqual(self.names("except:b,o,m,s,z"), ["Declarations.json"])
        self.assertEqual(self.names("first:2"), ["baseline.json", "outSimple.json"])
        self.assertEqual(
            self.names("files:z.json+first:1"), ["baseline.json", "z.json"]
        )
        self.assertEqual(self.names("shard:2/3"), ["outSimple.json", "SourcemapB.json"])
        self.assertEqual(self.names("prefix:S-Z"), ["SourcemapB.json", "z.json"])
        self.assertEqual(self.names("match:^(baseline|z)"), ["baseline.json", "z.json"])

    def test_extra_rows_and_bad_specs(self):
        table = dict(ps.SLICES, mine="startswith:z+first:1")
        self.assertEqual(self.names("mine", table), ["baseline.json", "z.json"])
        for bad in ("nope", "startswith:", "except:", "shard:4/3"):
            with self.assertRaises(ValueError):
                ps.select(bad, CASES, ps.SLICES)

    def test_grep_selects_exactly_the_tests_of_a_batch(self):
        cases = ps.cases_from_titles(entries("a.json", "ab.json"))
        grep = ps.grep_for([cases[0]]).replace("\\/", "/")
        hits = [t for c in cases for t in c.tests if re.search(grep, t)]
        self.assertEqual(hits, cases[0].tests)

    def test_batches(self):
        self.assertEqual(len(ps.batches(CASES, 4)), 2)


class ControlTest(unittest.TestCase):
    def tree(self, root, names):
        ts = Path(root, "ts")
        cases = ts / "tests" / "cases"
        (cases / "project").mkdir(parents=True)
        (cases / "projects").mkdir()
        ref = ts / "tests" / "baselines" / "reference" / "project"
        for n in names:
            (cases / "project" / f"{n}.json").write_text(
                json.dumps({"scenario": n, "projectRoot": "p"})
            )
            for kind in ("node", "amd"):
                d = ref / n / kind
                d.mkdir(parents=True)
                (d / f"{n}.json").write_text("{}")
                (d / f"{n}.errors.txt").write_text("err\n")
                (d / "test.js").write_text("js\n")
                (d / "test.d.ts").write_text("dts\n")
        return ts, ref

    def test_controls_edit_private_copies_only(self):
        with tempfile.TemporaryDirectory() as root:
            names = ("a", "b", "c", "d", "e", "f")
            ts, ref = self.tree(root, names)
            tests = ps.cases_from_titles(entries(*(f"{n}.json" for n in names)))
            work = Path(root, "work")
            work.mkdir()
            picked, rdir, cdir, done, expect = control.make_control(
                str(ts), tests[:2], tests[2:], str(work)
            )
            self.assertEqual([c.base for c in picked], [f"{n}.json" for n in "abcde"])
            self.assertEqual(expect, 6)
            self.assertEqual(
                done,
                [
                    "a/node/a.json: append-json",
                    "b/amd/b.errors.txt: replace-errors",
                    "c/amd/test.js: remove-js",
                    "d/amd/test.d.ts: append-dts",
                    "e.json: edit-case",
                ],
            )
            proj = Path(rdir) / "project"
            self.assertTrue(
                (proj / "a" / "node" / "a.json")
                .read_text()
                .endswith("corrupted reference\n")
            )
            self.assertFalse((proj / "c" / "amd" / "test.js").exists())
            self.assertTrue((proj / "c" / "node" / "test.js").exists())
            self.assertEqual((ref / "a" / "node" / "a.json").read_text(), "{}")
            self.assertFalse((proj / "f").exists())
            edited = json.loads((Path(cdir) / "project" / "e.json").read_text())
            self.assertEqual(edited["bug"], "control")
            self.assertTrue((Path(cdir) / "project" / "a.json").is_symlink())
            self.assertTrue((Path(cdir) / "projects").is_symlink())
            self.assertNotIn("bug", (ts / "tests/cases/project/e.json").read_text())

    def test_modes_without_a_matching_baseline_are_skipped(self):
        with tempfile.TemporaryDirectory() as root:
            ts, ref = self.tree(root, ("a",))
            for kind in ("node", "amd"):
                (ref / "a" / kind / "test.js").unlink()
            tests = ps.cases_from_titles(entries("a.json"))
            work = Path(root, "work")
            work.mkdir()
            picked, _, _, done, expect = control.make_control(
                str(ts), tests, [], str(work)
            )
            self.assertEqual(done, ["a/node/a.json: append-json"])
            self.assertEqual(expect, 1)


class OracleTest(unittest.TestCase):
    def test_known_failures_file_and_pin(self):
        self.assertTrue(oracle.KNOWN.endswith("project-1285-known-failures.txt"))
        self.assertEqual(oracle.pinned_total(), 2528)

    def test_skips_without_a_checkout(self):
        env = {k: v for k, v in os.environ.items() if k != "TS_SRC_DIR"}
        script = str(SCRIPTS_DIR / "run-ts-project-oracle.py")
        r = subprocess.run(
            [sys.executable, script], env=env, capture_output=True, text=True
        )
        self.assertEqual(r.returncode, 0)
        self.assertIn("SKIP", r.stdout)
        r = subprocess.run(
            [sys.executable, script, "--require"],
            env=env,
            capture_output=True,
            text=True,
        )
        self.assertEqual(r.returncode, 3)


if __name__ == "__main__":
    unittest.main()
