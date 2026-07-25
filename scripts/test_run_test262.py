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

    def run_runner(self, engine: Path, *extra_args: str) -> subprocess.CompletedProcess:
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
                "test262/test/sample.js",
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


if __name__ == "__main__":
    unittest.main()
