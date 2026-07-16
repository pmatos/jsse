import json
import os
import stat
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
HOOK = REPO_ROOT / "scripts" / "fmt-hook.sh"


class FmtHookTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.cargo_log = self.root / "cargo.log"
        fake_cargo = self.root / "cargo"
        fake_cargo.write_text(
            textwrap.dedent(
                """\
                #!/usr/bin/env bash
                printf '%s\\n' "$*" >> "$CARGO_LOG"
                if [[ "$1" == "fmt" && "${FAIL_FMT:-}" == "1" ]]; then
                    echo "fake rustfmt failure" >&2
                    exit 1
                fi
                if [[ "$1" == "clippy" && "${FAIL_CLIPPY:-}" == "1" ]]; then
                    echo "fake clippy failure" >&2
                    exit 1
                fi
                """
            ),
            encoding="utf-8",
        )
        fake_cargo.chmod(fake_cargo.stat().st_mode | stat.S_IXUSR)
        self.env = os.environ.copy()
        self.env["PATH"] = f"{self.root}:{self.env['PATH']}"
        self.env["CARGO_LOG"] = str(self.cargo_log)

    def tearDown(self):
        self.tmp.cleanup()

    def run_hook(
        self, file_path: Path, **extra_env: str
    ) -> subprocess.CompletedProcess:
        env = self.env | extra_env
        return subprocess.run(
            [str(HOOK)],
            input=json.dumps({"tool_input": {"file_path": str(file_path)}}),
            cwd=REPO_ROOT,
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )

    def cargo_commands(self) -> list[str]:
        if not self.cargo_log.exists():
            return []
        return self.cargo_log.read_text(encoding="utf-8").splitlines()

    def test_non_rust_file_is_a_noop(self):
        result = self.run_hook(REPO_ROOT / "README.md")

        self.assertEqual(result.returncode, 0)
        self.assertEqual(self.cargo_commands(), [])

    def test_source_file_formats_and_clippies_all_targets(self):
        result = self.run_hook(REPO_ROOT / "src" / "interpreter" / "eval.rs")

        self.assertEqual(result.returncode, 0)
        self.assertEqual(
            self.cargo_commands(),
            [
                f"fmt --manifest-path {REPO_ROOT}/Cargo.toml -- "
                f"{REPO_ROOT}/src/interpreter/eval.rs",
                f"clippy --manifest-path {REPO_ROOT}/Cargo.toml --quiet "
                "--all-targets -- -D warnings",
            ],
        )

    def test_cfg_test_source_is_clippied_with_all_targets(self):
        # A #[cfg(test)]-gated module (e.g. src/interpreter/tests.rs) is not
        # compiled by --bin jsse, so it must go through --all-targets or the
        # hook would report success without ever linting the edit.
        result = self.run_hook(
            REPO_ROOT / "src" / "interpreter" / "tests.rs"
        )

        self.assertEqual(result.returncode, 0)
        self.assertIn("--all-targets -- -D warnings", self.cargo_commands()[1])

    def test_integration_test_uses_its_named_target(self):
        result = self.run_hook(REPO_ROOT / "tests" / "test262_smoke_oracle.rs")

        self.assertEqual(result.returncode, 0)
        self.assertIn(
            "--test test262_smoke_oracle -- -D warnings",
            self.cargo_commands()[1],
        )

    def test_rust_file_outside_a_cargo_target_is_only_formatted(self):
        rust_file = self.root / "standalone.rs"
        rust_file.touch()

        result = self.run_hook(rust_file)

        self.assertEqual(result.returncode, 0)
        self.assertEqual(len(self.cargo_commands()), 1)
        self.assertTrue(self.cargo_commands()[0].startswith("fmt "))

    def test_clippy_failure_is_returned_as_post_tool_feedback(self):
        result = self.run_hook(REPO_ROOT / "src" / "main.rs", FAIL_CLIPPY="1")

        self.assertEqual(result.returncode, 2)
        self.assertIn("Clippy failed for src/main.rs", result.stderr)
        self.assertIn("fake clippy failure", result.stderr)

    def test_rustfmt_failure_stops_before_clippy(self):
        result = self.run_hook(REPO_ROOT / "src" / "main.rs", FAIL_FMT="1")

        self.assertEqual(result.returncode, 2)
        self.assertIn("rustfmt failed", result.stderr)
        self.assertIn("fake rustfmt failure", result.stderr)
        self.assertEqual(len(self.cargo_commands()), 1)


if __name__ == "__main__":
    unittest.main()
