import importlib.util
import os
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))
import ts_lint_cases as lc  # noqa: E402

spec = importlib.util.spec_from_file_location(
    "run_ts_lint_oracle", SCRIPTS_DIR / "run-ts-lint-oracle.py"
)
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)


def result(stdout="", exit_code="0", changed=None, stderr=""):
    step = {"exit": exit_code, "stdout": stdout, "stderr": stderr}
    return {"steps": [step], "changed": changed or {}}


class CaseTableTest(unittest.TestCase):
    def test_names_are_unique_and_every_control_has_a_fixture(self):
        table = lc.cases()
        names = [c.name for c in table]
        self.assertEqual(len(names), len(set(names)))
        for c in table:
            for rel in c.controls:
                fixture = SCRIPTS_DIR / "ts-lint-fixtures" / lc.FIXTURES[rel]
                self.assertTrue(fixture.is_file(), fixture)

    def test_every_case_has_a_check_that_it_found_something(self):
        for c in lc.cases():
            self.assertTrue(c.must, c.name)

    def test_full_is_named_only_and_the_default_leaves_it_out(self):
        table = lc.cases()
        default = [c.name for c in lc.select([], table)]
        self.assertNotIn("full", default)
        self.assertEqual([c.name for c in lc.select(["full"], table)], ["full"])
        self.assertIn("full", [c.name for c in lc.select(["all"], table)])

    def test_select_takes_globs_and_cheap_and_rejects_unknown_names(self):
        table = lc.cases()
        self.assertEqual(
            [c.name for c in lc.select(["print-config-*"], table)],
            ["print-config-src", "print-config-scripts", "print-config-dts"],
        )
        self.assertTrue(all(c.cheap for c in lc.select(["cheap"], table)))
        with self.assertRaises(SystemExit):
            lc.select(["nope"], table)

    def test_the_lint_task_flags_are_the_herebyfiles(self):
        self.assertEqual(
            lc.LINT_FLAGS, ["--report-unused-disable-directives", "--max-warnings", "0"]
        )


class NormaliseTest(unittest.TestCase):
    def test_tree_paths_durations_the_interpreter_and_mtimes_are_dropped(self):
        text = (
            "Using /w/ts/Herebyfile.mjs to run lint\n"
            "> /bin/jsse node_modules/eslint/bin/eslint .\n"
            "Finished lint in 35.5s\n"
            '{"size": 3, "mtime": 1793084413917}\n'
            "Generated on Fri Oct 30 2026 (CET)"
        )
        out = oracle.normalise(text, "/w/ts", "/bin/jsse")
        self.assertEqual(
            out.splitlines(),
            [
                "Using <TREE>/Herebyfile.mjs to run lint",
                "> <EXEC> node_modules/eslint/bin/eslint .",
                "Finished lint in <T>",
                '{"size": 3, "mtime": 0}',
                "Generated on <DATE>",
            ],
        )


class CacheTest(unittest.TestCase):
    def cache(self, config_a, config_b, path="/w/ts/a.ts"):
        entries = [
            {path: "1", "/w/ts/b.ts": "2", "/w/ts/c.ts": "3"},
            {"size": 1, "hashOfConfig": "4"},
            {"size": 2, "hashOfConfig": "5"},
            {"size": 3, "hashOfConfig": "4"},
            config_a,
            config_b,
        ]
        return oracle.json.dumps(entries)

    def test_config_hashes_are_compared_as_a_partition_not_by_value(self):
        one = oracle.normalise_cache(self.cache("aaa", "bbb"))
        two = oracle.normalise_cache(self.cache("xyz", "qrs"))
        self.assertEqual(one, two)

    def test_files_that_stop_sharing_a_config_are_a_difference(self):
        entries = oracle.json.loads(self.cache("aaa", "bbb"))
        entries[3]["hashOfConfig"] = "5"
        merged = oracle.json.dumps(entries)
        self.assertNotEqual(
            oracle.normalise_cache(merged),
            oracle.normalise_cache(self.cache("aaa", "bbb")),
        )

    def test_text_that_is_not_json_is_left_alone(self):
        self.assertEqual(oracle.normalise_cache("not json"), "not json")


class CompareTest(unittest.TestCase):
    def test_identical_results_compare_equal(self):
        a = result("x\n", "1", {"f": "body"})
        self.assertEqual(oracle.compare(a, result("x\n", "1", {"f": "body"})), [])

    def test_each_kind_of_divergence_is_reported(self):
        base = result("out", "1", {"f": "a"})
        self.assertTrue(oracle.compare(base, result("out", "0", {"f": "a"})))
        self.assertTrue(oracle.compare(base, result("other", "1", {"f": "a"})))
        self.assertTrue(oracle.compare(base, result("out", "1", {"f": "b"})))
        self.assertTrue(oracle.compare(base, result("out", "1", {})))
        self.assertTrue(oracle.compare(base, result("out", "1", {"f": "a", "g": "c"})))
        self.assertTrue(oracle.compare(base, result("out", "1", {"f": "a"}, "err")))


class VerdictTest(unittest.TestCase):
    case = lc.Case("c", [], must=(("exit", r"^1$"), ("stdout", r"no-var")))

    def verdict(self, node, jsse, known=None):
        return oracle.verdict(self.case, node, jsse, known or {})[0]

    def test_a_match_passes(self):
        ok = result("no-var", "1")
        self.assertEqual(self.verdict(ok, ok), "PASS")

    def test_a_vacuous_control_fails_even_when_both_engines_agree(self):
        empty = result("", "0")
        self.assertEqual(self.verdict(empty, empty), "FAIL")

    def test_a_difference_fails_unless_listed_and_a_stale_listing_fails(self):
        ok, other = result("no-var", "1"), result("no-var more", "1")
        self.assertEqual(self.verdict(ok, other), "FAIL")
        self.assertEqual(self.verdict(ok, other, {"c": "#1"}), "KNOWN")
        self.assertEqual(self.verdict(ok, ok, {"c": "#1"}), "STALE")

    def test_must_can_look_at_the_files_a_step_wrote(self):
        case = lc.Case("c", [], must=(("file a/b", r"const x"),))
        good = result(changed={"a/b": "const x = 1;"})
        self.assertEqual(oracle.check_must(case, good), [])
        self.assertTrue(oracle.check_must(case, result(changed={"a/b": "let x"})))
        self.assertTrue(oracle.check_must(case, result()))


class FilesTest(unittest.TestCase):
    def test_known_failures_file_format(self):
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "k.txt")
            with open(path, "w", encoding="utf-8") as f:
                f.write("# comment\n\nfull\t#1400\n")
            self.assertEqual(oracle.load_known(path), {"full": "#1400"})
            self.assertEqual(oracle.load_known(os.path.join(d, "none")), {})

    def test_the_light_tree_links_the_heavy_directories_and_installs_controls(self):
        with tempfile.TemporaryDirectory() as d:
            ts = os.path.join(d, "ts")
            for sub in ("node_modules", "tests", "src/compiler", "scripts"):
                os.makedirs(os.path.join(ts, sub))
            Path(ts, "eslint.config.mjs").write_text("export default [];\n")
            tree = os.path.join(d, "tree")
            case = lc.Case("c", [], controls=(lc.MJS,))
            oracle.make_tree(ts, tree, case)
            self.assertTrue(os.path.islink(os.path.join(tree, "node_modules")))
            self.assertFalse(os.path.islink(os.path.join(tree, "src")))
            self.assertTrue(os.path.isfile(os.path.join(tree, "eslint.config.mjs")))
            control = os.path.join(tree, lc.MJS)
            self.assertTrue(os.path.isfile(control))
            self.assertEqual(os.stat(control).st_mtime, oracle.FIXED_MTIME)
            shots = oracle.snapshot(tree)
            self.assertIn(lc.MJS, shots)
            self.assertNotIn("node_modules", " ".join(shots))

    def test_argv_for_each_tool(self):
        eslint = oracle.argv_of(lc.lint("x.ts"), "/e", True)
        self.assertEqual(
            eslint[:3], ["/e", "--node", "node_modules/eslint/bin/eslint.js"]
        )
        self.assertEqual(eslint[3:], [*lc.LINT_FLAGS, "x.ts"])
        plain = oracle.argv_of(lc.lint("--version", plain=True), "node", False)
        self.assertEqual(plain[1:], ["node_modules/eslint/bin/eslint.js", "--version"])
        self.assertEqual(
            oracle.argv_of(lc.hereby("lint"), "/e", True),
            ["/e", "--node", "node_modules/hereby/bin/hereby.js", "lint"],
        )

    def test_without_ts_src_dir_the_oracle_skips_unless_required(self):
        saved = os.environ.pop("TS_SRC_DIR", None)
        argv = sys.argv
        try:
            sys.argv = ["run-ts-lint-oracle.py"]
            self.assertEqual(oracle.main(), 0)
            sys.argv = ["run-ts-lint-oracle.py", "--require"]
            self.assertEqual(oracle.main(), 1)
        finally:
            sys.argv = argv
            if saved is not None:
                os.environ["TS_SRC_DIR"] = saved


if __name__ == "__main__":
    unittest.main()
