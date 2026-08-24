import os
import stat
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
RUNNER = REPO_ROOT / "scripts" / "run-test262.py"


class RunTest262ExitStatusTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.test262 = self.root / "test262"
        test_dir = self.test262 / "test"
        test_dir.mkdir(parents=True)
        self.test_file = test_dir / "sample.js"
        self.test_file.write_text(
            textwrap.dedent(
                """\
                /*---
                flags: [raw]
                ---*/
                """
            ),
            encoding="utf-8",
        )

    def tearDown(self):
        self.tmp.cleanup()

    def write_engine(self, exit_code: int) -> Path:
        engine = self.root / f"engine_exit_{exit_code}.py"
        engine.write_text(
            textwrap.dedent(
                f"""\
                #!{sys.executable}
                import sys
                sys.exit({exit_code})
                """
            ),
            encoding="utf-8",
        )
        engine.chmod(engine.stat().st_mode | stat.S_IXUSR)
        return engine

    def run_runner(
        self,
        engine: Path,
        *extra_args: str,
        paths: tuple[str, ...] = ("test262/test/sample.js",),
    ) -> subprocess.CompletedProcess:
        return subprocess.run(
            [
                sys.executable,
                str(RUNNER),
                "--jsse",
                str(engine),
                "--test262",
                "test262",
                "--baseline-ref",
                "refs/does-not-exist",
                "-j",
                "1",
                *extra_args,
                *paths,
            ],
            cwd=self.root,
            env={**os.environ, "TZ": "America/New_York"},
            text=True,
            capture_output=True,
            check=False,
        )

    def test_fail_on_failures_exits_non_zero_for_non_regression_failures(self):
        result = self.run_runner(self.write_engine(1), "--fail-on-failures")

        self.assertEqual(result.returncode, 1)
        self.assertIn("Fail:    1", result.stdout)
        self.assertIn("FAILED: test262/test/sample.js", result.stdout)
        self.assertIn("Error: 1 test262 scenario(s) failed.", result.stderr)

    def test_report_mode_allows_non_regression_failures(self):
        result = self.run_runner(self.write_engine(1))

        self.assertEqual(result.returncode, 0)
        self.assertIn("Fail:    1", result.stdout)

    def test_baseline_regressions_exit_non_zero_in_report_mode(self):
        (self.root / "test262-pass.txt").write_text(
            "test262/test/sample.js\n",
            encoding="utf-8",
        )

        result = self.run_runner(self.write_engine(1))

        self.assertEqual(result.returncode, 1)
        self.assertIn("REGRESSED: test262/test/sample.js", result.stdout)
        self.assertIn("Error: 1 baseline regression(s) detected.", result.stderr)

    def test_child_engine_runs_in_utc(self):
        engine = self.root / "engine_timezone.py"
        engine.write_text(
            textwrap.dedent(
                f"""\
                #!{sys.executable}
                import os
                import sys
                sys.exit(0 if os.environ.get("TZ") == "UTC" else 1)
                """
            ),
            encoding="utf-8",
        )
        engine.chmod(engine.stat().st_mode | stat.S_IXUSR)

        result = self.run_runner(engine, "--fail-on-failures")

        self.assertEqual(result.returncode, 0)

    def test_explicit_non_fixture_mjs_is_rejected(self):
        extra_dir = self.root / "test262-extra"
        extra_dir.mkdir()
        (extra_dir / "module-test.mjs").write_text("", encoding="utf-8")

        result = self.run_runner(
            self.write_engine(0), paths=("test262-extra/module-test.mjs",)
        )

        self.assertEqual(result.returncode, 2)
        self.assertIn("non-fixture .mjs files would not be collected", result.stderr)
        self.assertIn("test262-extra/module-test.mjs", result.stderr)
        self.assertIn("flags: [module]", result.stderr)

    def test_directory_with_non_fixture_mjs_is_rejected(self):
        extra_dir = self.root / "test262-extra"
        nested_dir = extra_dir / "nested"
        nested_dir.mkdir(parents=True)
        (nested_dir / "module-test.mjs").write_text("", encoding="utf-8")

        result = self.run_runner(self.write_engine(0), paths=("test262-extra",))

        self.assertEqual(result.returncode, 2)
        self.assertIn("test262-extra/nested/module-test.mjs", result.stderr)

    def test_directory_allows_mjs_fixtures(self):
        extra_dir = self.root / "test262-extra"
        extra_dir.mkdir()
        (extra_dir / "module-test.js").write_text(
            textwrap.dedent(
                """\
                /*---
                flags: [module]
                ---*/
                """
            ),
            encoding="utf-8",
        )
        (extra_dir / "dep_FIXTURE.mjs").write_text("", encoding="utf-8")

        result = self.run_runner(
            self.write_engine(0),
            "--fail-on-failures",
            paths=("test262-extra",),
        )

        self.assertEqual(result.returncode, 0)
        self.assertIn("Files:   1", result.stdout)

    def test_test262_submodule_mjs_does_not_block_collection(self):
        tools_dir = self.test262 / "tools"
        tools_dir.mkdir(parents=True)
        (tools_dir / "generator.mjs").write_text("", encoding="utf-8")

        result = self.run_runner(
            self.write_engine(0), "--fail-on-failures", paths=("test262",)
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("would not be collected", result.stderr)


if __name__ == "__main__":
    unittest.main()
