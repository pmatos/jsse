import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent
ORACLE = SCRIPTS / "run-tsc-selfcompile-oracle.py"

spec = importlib.util.spec_from_file_location("run_tsc_selfcompile_oracle", ORACLE)
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)


def run(secs=1.0, rss=100.0, files=None, exit_code="0", out=""):
    return {
        "exit": exit_code,
        "stdout": out,
        "stderr": "",
        "files": {"built/a.js": "x"} if files is None else files,
        "seconds": secs,
        "rss_mb": rss,
    }


class CompareTest(unittest.TestCase):
    def test_identical_runs(self):
        self.assertEqual(oracle.compare(run(), run(secs=50)), [])

    def test_differing_missing_and_extra_files(self):
        node = run(files={"a": "1", "b": "2", "c": "3"})
        jsse = run(files={"a": "1", "b": "9", "d": "4"})
        self.assertEqual(
            oracle.compare(node, jsse),
            ["differs: b", "only on node: c", "only on jsse: d"],
        )

    def test_exit_and_output_must_agree(self):
        lines = oracle.compare(run(), run(exit_code="2", out="error TS1"))
        self.assertTrue(any(line.startswith("exit differs") for line in lines))
        self.assertTrue(any(line.startswith("stdout differs") for line in lines))

    def test_a_failing_node_run_or_no_output_is_an_error(self):
        failing = run(exit_code="2")
        self.assertTrue(oracle.compare(failing, failing))
        self.assertEqual(
            oracle.compare(run(files={}), run(files={})), ["no files were emitted"]
        )


class BudgetTest(unittest.TestCase):
    def test_no_budget_means_no_limit(self):
        self.assertEqual(
            oracle.check_budget("c", run(), run(secs=1e6, rss=1e6), {}), []
        )
        nulls = {"c": {"jsse_max_seconds": None, "max_time_ratio": None}}
        self.assertEqual(oracle.check_budget("c", run(), run(secs=1e6), nulls), [])

    def test_absolute_limits(self):
        budget = {"c": {"jsse_max_seconds": 10, "jsse_max_rss_mb": 500}}
        self.assertEqual(
            oracle.check_budget("c", run(), run(secs=10, rss=500), budget), []
        )
        over = oracle.check_budget("c", run(), run(secs=11, rss=501), budget)
        self.assertEqual(len(over), 2)
        self.assertIn("wall time", over[0])
        self.assertIn("peak RSS", over[1])

    def test_ratio_limits_are_relative_to_node(self):
        budget = {"c": {"max_time_ratio": 100, "max_rss_ratio": 3}}
        node = run(secs=2, rss=100)
        self.assertEqual(
            oracle.check_budget("c", node, run(secs=200, rss=300), budget), []
        )
        over = oracle.check_budget("c", node, run(secs=201, rss=301), budget)
        self.assertEqual(len(over), 2)

    def test_command_line_limits_override_the_file(self):
        budget = {"c": {"jsse_max_seconds": 1000}}
        over = oracle.check_budget("c", run(), run(secs=20), budget, max_seconds=10)
        self.assertEqual(len(over), 1)
        over = oracle.check_budget("c", run(), run(rss=20), {}, max_rss_mb=10)
        self.assertEqual(len(over), 1)

    def test_shipped_budget_file_names_known_cases_with_known_keys(self):
        budget = oracle.load_budget(oracle.BUDGET)
        self.assertTrue(set(budget) <= set(oracle.CASES))
        keys = {
            "jsse_max_seconds",
            "jsse_max_rss_mb",
            "max_time_ratio",
            "max_rss_ratio",
        }
        for cfg in budget.values():
            self.assertTrue(set(cfg) <= keys)

    def test_shipped_budget_is_an_rss_ratio_cap_with_no_wall_time_cap(self):
        budget = oracle.load_budget(oracle.BUDGET)
        self.assertEqual(set(budget), set(oracle.CASES))
        for name, cfg in budget.items():
            self.assertEqual(cfg.get("max_rss_ratio"), 6, name)
            for key in ("jsse_max_seconds", "jsse_max_rss_mb", "max_time_ratio"):
                self.assertIsNone(cfg.get(key), f"{name}.{key}")


class GeneratorTest(unittest.TestCase):
    def test_program_is_deterministic_and_sized(self):
        with tempfile.TemporaryDirectory() as a, tempfile.TemporaryDirectory() as b:
            oracle.gen_program(a, 5)
            oracle.gen_program(b, 5)
            names = sorted(os.listdir(a))
            self.assertEqual(names, [f"m{i}.ts" for i in range(5)])
            for n in names:
                self.assertEqual((Path(a) / n).read_bytes(), (Path(b) / n).read_bytes())
            self.assertIn('from "./m3.js"', (Path(a) / "m4.ts").read_text())

    def test_cases_reference_known_pieces(self):
        for name, (generated, args) in oracle.CASES.items():
            self.assertIn("--outDir", args, name)
            self.assertEqual(generated, "gen" in args, name)
        self.assertTrue(set(oracle.SMOKE) <= set(oracle.CASES))


class PrepareTest(unittest.TestCase):
    def test_src_copy_drops_test_dirs_links_node_modules_and_replaces_a_stale_tree(
        self,
    ):
        with tempfile.TemporaryDirectory() as d:
            ts = Path(d, "ts")
            for rel in ("src/compiler/a.ts", "src/testRunner/t.ts", "src/harness/h.ts"):
                (ts / rel).parent.mkdir(parents=True, exist_ok=True)
                (ts / rel).write_text("x")
            (ts / "node_modules").mkdir()
            tree = Path(d, "tree")
            tree.mkdir()
            (tree / "stale").write_text("old")
            oracle.prepare(str(tree), str(ts), False)
            self.assertEqual(
                sorted(os.listdir(tree / "src")),
                ["compiler", "tsconfig.selfcompile.json"],
            )
            self.assertTrue((tree / "node_modules").is_symlink())
            self.assertFalse((tree / "stale").exists())
            config = json.loads(
                (tree / "src" / "tsconfig.selfcompile.json").read_text()
            )
            self.assertEqual(config["extends"], "./tsconfig-base")

    def test_generated_tree_has_its_own_config_and_no_src(self):
        with tempfile.TemporaryDirectory() as d:
            oracle.prepare(os.path.join(d, "t"), None, True)
            self.assertEqual(sorted(os.listdir(Path(d, "t"))), ["gen"])
            self.assertTrue(Path(d, "t", "gen", "tsconfig.json").is_file())


class InputsTest(unittest.TestCase):
    def package(self, root, version):
        (root / "lib").mkdir(parents=True, exist_ok=True)
        (root / "lib" / "_tsc.js").write_text("")
        (root / "package.json").write_text(
            json.dumps({"name": "typescript", "version": version})
        )
        return str(root)

    def test_generated_cases_need_no_checkout(self):
        with tempfile.TemporaryDirectory() as d:
            tsc = self.package(Path(d), "6.0.3")
            js, problem = oracle.check_inputs(None, tsc, False)
            self.assertEqual((js, problem), (os.path.join(tsc, "lib", "_tsc.js"), None))
            js, problem = oracle.check_inputs(None, tsc, True)
            self.assertIn("TS_SRC_DIR is not set", problem)

    def test_wrong_compiler_version_is_refused(self):
        with tempfile.TemporaryDirectory() as d:
            _, problem = oracle.check_inputs(
                None, self.package(Path(d), "5.9.3"), False
            )
            self.assertIn("not an installed typescript 6.0.3", problem)


class NormaliseTest(unittest.TestCase):
    def test_tree_path_is_replaced_in_every_spelling(self):
        home = os.path.expanduser("~")
        tree = home + "/w/tree"
        data = f"{tree}/a.ts ~/w/tree/b.ts".encode()
        self.assertEqual(oracle.normalise(data, tree), b"<TREE>/a.ts <TREE>/b.ts")


class RunMeasuredTest(unittest.TestCase):
    def test_exit_output_and_rss(self):
        code, out, err, secs, rss = oracle.run_measured(
            [
                sys.executable,
                "-c",
                "import sys; print('o'); print('e', file=sys.stderr); sys.exit(3)",
            ],
            os.getcwd(),
            dict(os.environ),
            60,
        )
        self.assertEqual((code, out, err), (3, b"o\n", b"e\n"))
        self.assertGreater(rss, 1)
        self.assertGreaterEqual(secs, 0)

    def test_large_output_does_not_block(self):
        code, out, _, _, _ = oracle.run_measured(
            [sys.executable, "-c", "print('x' * 5_000_000)"],
            os.getcwd(),
            dict(os.environ),
            60,
        )
        self.assertEqual(code, 0)
        self.assertEqual(len(out), 5_000_001)

    def test_timeout_kills_the_process_group(self):
        code, _, _, secs, _ = oracle.run_measured(
            ["sh", "-c", "sleep 30 & wait"], os.getcwd(), dict(os.environ), 1
        )
        self.assertEqual(code, "timeout")
        self.assertLess(secs, 20)


class SkipTest(unittest.TestCase):
    def run_script(self, *args, **env):
        base = {
            k: v for k, v in os.environ.items() if k not in ("TS_SRC_DIR", "TSC_DIR")
        }
        return subprocess.run(
            [sys.executable, str(ORACLE), *args],
            env={**base, **env},
            capture_output=True,
            text=True,
        )

    def test_skips_without_a_checkout_unless_required(self):
        r = self.run_script()
        self.assertEqual(r.returncode, 0)
        self.assertIn("SKIP: TS_SRC_DIR is not set", r.stdout)
        self.assertEqual(self.run_script("--require").returncode, 1)

    def test_skips_on_another_typescript_version(self):
        with tempfile.TemporaryDirectory() as d:
            Path(d, "package.json").write_text(
                json.dumps({"name": "typescript", "version": "5.9.3"})
            )
            r = self.run_script(TS_SRC_DIR=d)
            self.assertEqual(r.returncode, 0)
            self.assertIn("not 6.0.3", r.stdout)

    def test_smoke_needs_only_a_compiler(self):
        r = self.run_script("--smoke")
        self.assertIn("SKIP: TSC_DIR is not set", r.stdout)
        with tempfile.TemporaryDirectory() as d:
            Path(d, "package.json").write_text(
                json.dumps({"name": "typescript", "version": "6.0.3"})
            )
            r = self.run_script("--smoke", TSC_DIR=d)
            self.assertIn("_tsc.js is missing", r.stdout)

    def test_unknown_case_is_an_error(self):
        self.assertEqual(self.run_script("no-such-case").returncode, 2)


if __name__ == "__main__":
    unittest.main()
