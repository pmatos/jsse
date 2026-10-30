import importlib.util
import re
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
NAMES = (
    "baseline.json",
    "outSimple.json",
    "mapRootAbsolutePathX.json",
    "maprootUrlsourcerootUrlY.json",
    "sourcerootUrlZ.json",
    "sourcemapW.json",
    "baseline2.json",
    "declarationDir.json",
)


def entries(names=NAMES):
    out = []
    for n in names:
        for mod in ("@module: commonjs", "@module: amd"):
            key = f"projects tests Compiling project for [Scen]/[Ario]: {n}: testcase tests/cases/project/{n} ({mod})"
            out += [
                (
                    f"{key} {t} for tests/cases/project/{n}",
                    f"{t} for tests/cases/project/{n}",
                )
                for t in ITS
            ]
    return out


SUITES = ps.suites_from_titles(entries())


class SliceTest(unittest.TestCase):
    def files(self, text, table=None):
        return sorted({s.base for s in ps.select(text, SUITES, table or ps.SLICES)})

    def test_a_suite_is_one_file_and_module_kind_with_four_tests(self):
        self.assertEqual(len(SUITES), 2 * len(NAMES))
        s = SUITES[0]
        self.assertEqual((s.base, s.config), ("baseline.json", "@module: commonjs"))
        self.assertEqual(len(s.tests), 4)
        self.assertEqual(SUITES[1].config, "@module: amd")

    def test_foreign_titles_are_ignored(self):
        other = [("compiler tests for a.ts Correct errors", "Correct errors")]
        self.assertEqual(ps.suites_from_titles(other), [])

    def test_the_matrix_row_is_the_issue_selection_and_nothing_else(self):
        self.assertEqual(
            self.files("matrix"),
            [
                "mapRootAbsolutePathX.json",
                "maprootUrlsourcerootUrlY.json",
                "outSimple.json",
                "sourcemapW.json",
                "sourcerootUrlZ.json",
            ],
        )

    def test_family_rows_partition_the_matrix(self):
        fam = ("out", "sourcemap", "maproot", "sourceroot")
        got = sorted(n for r in fam for n in self.files(r))
        self.assertEqual(got, self.files("matrix"))

    def test_terms_and_unions(self):
        self.assertEqual(len(ps.select("first:3", SUITES)), 3)
        self.assertEqual(
            self.files("files:baseline.json+prefix:D"),
            ["baseline.json", "declarationDir.json"],
        )
        self.assertEqual(len(ps.select("shard:1/2", SUITES)), len(NAMES))
        self.assertEqual(len(ps.select("all", SUITES, ps.SLICES)), len(SUITES))
        with self.assertRaises(ValueError):
            ps.select("nope", SUITES)

    def test_grep_selects_exactly_the_tests_of_a_batch(self):
        grep = ps.grep_for([SUITES[0]])
        titles = [t for s in SUITES for t in s.tests]
        hits = [t for t in titles if re.search(grep.replace("\\/", "/"), t)]
        self.assertEqual(hits, SUITES[0].tests)


class ControlTest(unittest.TestCase):
    def test_references_are_corrupted_in_a_private_copy_only(self):
        with tempfile.TemporaryDirectory() as root:
            ref, dest = Path(root, "ref"), Path(root, "dest")
            dest.mkdir()
            for n in ("outSimple", "sourcemapW", "baseline"):
                for mod in ("node", "amd"):
                    d = ref / "project" / n / mod
                    (d / "ref").mkdir(parents=True)
                    for f in (f"{n}.json", "test.js", "test.js.map", "test.d.ts"):
                        (d / f).write_text("ref\n")
            suites = [
                s
                for s in SUITES
                if s.base in ("outSimple.json", "sourcemapW.json", "baseline.json")
            ]
            picked, done = control.make_control(str(ref), suites, str(dest))
            self.assertEqual(
                [s.base for s in picked],
                ["baseline.json", "outSimple.json", "sourcemapW.json"],
            )
            self.assertEqual(
                done,
                [
                    "baseline/node/baseline.json: append",
                    "outSimple/node/test.js: replace",
                    "sourcemapW/node/test.js.map: remove",
                ],
            )
            self.assertTrue(
                (dest / "project/baseline/node/baseline.json")
                .read_text()
                .endswith("corrupted reference\n")
            )
            self.assertFalse((dest / "project/sourcemapW/node/test.js.map").exists())
            self.assertEqual(
                (ref / "project/baseline/node/baseline.json").read_text(), "ref\n"
            )

    def test_suites_without_references_are_skipped(self):
        with tempfile.TemporaryDirectory() as root:
            self.assertEqual(control.make_control(root, SUITES, root), ([], []))


class OracleTest(unittest.TestCase):
    def test_pinned_count_only_for_the_whole_runner(self):
        self.assertEqual(oracle.pinned_expected(SUITES, SUITES), 2528)
        self.assertIsNone(oracle.pinned_expected(SUITES[:2], SUITES))

    def test_the_known_failures_file_exists_and_parses(self):
        self.assertIsInstance(oracle.load_known(oracle.KNOWN), dict)

    def test_checkout_problem_without_a_checkout(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertIn("not a built TypeScript", oracle.checkout_problem(d))


if __name__ == "__main__":
    unittest.main()
