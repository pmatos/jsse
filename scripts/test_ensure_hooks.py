import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import textwrap
import unittest


REPO_ROOT = Path(__file__).resolve().parents[1]


class EnsureHooksTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name) / "repo with spaces"
        self.root.mkdir()
        self.env = os.environ.copy()
        for key in (
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_COMMON_DIR",
            "SKIP",
        ):
            self.env.pop(key, None)
        self.env["GIT_CONFIG_GLOBAL"] = "/dev/null"
        self.env["GIT_CONFIG_NOSYSTEM"] = "1"
        self.git("init", "--quiet")
        self.scripts = self.root / "scripts"
        self.scripts.mkdir()
        for name in ("ensure-hooks.sh", "agent-session.sh", "require-agent-hooks.sh"):
            shutil.copy2(REPO_ROOT / "scripts" / name, self.scripts)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.log = self.root / "calls.log"
        self.env["PATH"] = f"{self.bin}:{self.env['PATH']}"
        self.env["HOOK_TEST_LOG"] = str(self.log)
        fake = self.bin / "pre-commit"
        fake.write_text(
            textwrap.dedent(
                """\
                #!/usr/bin/env bash
                set -euo pipefail
                echo "$*" >> "$HOOK_TEST_LOG"
                [[ "${FAIL_SETUP:-}" != 1 ]] || exit 1
                if [[ "$1" == install && "${OMIT_HOOKS:-}" != 1 ]]; then
                    for hook in pre-commit commit-msg; do
                        path=$(git rev-parse --git-path "hooks/$hook")
                        mkdir -p "$(dirname "$path")"
                        printf '#!/bin/sh\\n# pre_commit\\nexit 0\\n' > "$path"
                        chmod +x "$path"
                    done
                fi
                """
            )
        )
        fake.chmod(0o755)

    def git(self, *args: str) -> bytes:
        return subprocess.run(
            ["git", *args],
            cwd=self.root,
            env=self.env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=True,
        ).stdout

    def run_setup(self, *args: str, **env: str) -> subprocess.CompletedProcess:
        return subprocess.run(
            [str(self.scripts / "ensure-hooks.sh"), *args],
            cwd=self.root,
            env=self.env | env,
            text=True,
            capture_output=True,
        )

    def calls(self) -> list[str]:
        return self.log.read_text().splitlines() if self.log.exists() else []

    def test_installs_both_hooks_and_prepares_environments(self):
        result = self.run_setup()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls(), ["install --install-hooks"])
        for name in ("pre-commit", "commit-msg"):
            self.assertTrue(os.access(self.root / ".git" / "hooks" / name, os.X_OK))

    def test_resume_does_not_rewrite_installed_hooks(self):
        self.assertEqual(self.run_setup().returncode, 0)
        hook = self.root / ".git" / "hooks" / "pre-commit"
        original = hook.stat().st_mtime_ns
        result = self.run_setup()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls(), ["install --install-hooks", "install-hooks"])
        self.assertEqual(hook.stat().st_mtime_ns, original)

    def test_readiness_check_never_installs_or_rewrites_hooks(self):
        self.assertNotEqual(self.run_setup("--check").returncode, 0)
        self.assertEqual(self.calls(), [])
        self.assertEqual(self.run_setup().returncode, 0)
        self.assertEqual(self.run_setup("--check").returncode, 0)
        self.assertEqual(self.calls(), ["install --install-hooks"])

    def test_feature_tools_are_blocked_until_hooks_are_enabled(self):
        def check():
            return subprocess.run(
                [str(self.scripts / "require-agent-hooks.sh")],
                cwd=self.root,
                env=self.env,
                text=True,
                capture_output=True,
            )

        result = check()
        self.assertEqual(result.returncode, 2)
        self.assertIn("Feature work blocked", result.stderr)
        self.assertEqual(self.run_setup().returncode, 0)
        self.assertEqual(check().returncode, 0)
        (self.root / ".git" / "hooks" / "pre-commit").unlink()
        self.assertEqual(check().returncode, 2)

    def test_setup_failure_explains_required_remedy(self):
        result = self.run_setup(FAIL_SETUP="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Do not commit", result.stderr)
        self.assertIn("read-only", result.stderr)
        self.assertIn("PRE_COMMIT_HOME", result.stderr)

    def test_successful_installer_with_missing_hooks_still_fails(self):
        result = self.run_setup(OMIT_HOOKS="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("must both be executable", result.stderr)

    def test_partial_installation_is_repaired(self):
        self.assertEqual(self.run_setup().returncode, 0)
        (self.root / ".git" / "hooks" / "commit-msg").unlink()
        self.assertEqual(self.run_setup().returncode, 0)
        self.assertEqual(self.calls(), ["install --install-hooks"] * 2)

    def test_linked_worktree_uses_shared_git_hooks(self):
        self.git("config", "user.name", "Hook Test")
        self.git("config", "user.email", "hook-test@example.invalid")
        self.git("config", "commit.gpgsign", "false")
        self.git("add", "--", "scripts")
        self.git("commit", "--quiet", "-m", "fixture")
        linked = Path(self.tmp.name) / "linked worktree"
        self.git("worktree", "add", "--quiet", "--detach", str(linked), "HEAD")
        result = subprocess.run(
            [
                str(self.scripts / "agent-session.sh"),
                "--worktree",
                str(linked),
                "git",
                "rev-parse",
                "--show-toplevel",
            ],
            cwd=self.root,
            env=self.env,
            text=True,
            capture_output=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.splitlines()[-1], str(linked))
        self.assertTrue((self.root / ".git" / "hooks" / "pre-commit").exists())

    def test_skip_and_overridden_hook_path_are_rejected(self):
        self.assertNotEqual(
            self.run_setup(SKIP="staged-file-token-limit").returncode, 0
        )
        self.git("config", "core.hooksPath", "/dev/null")
        result = self.run_setup()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("core.hooksPath", result.stderr)
        self.assertEqual(self.calls(), [])
        self.git("config", "core.hooksPath", "")
        self.assertNotEqual(self.run_setup().returncode, 0)

    def test_missing_pre_commit_reports_install_command(self):
        minimal_bin = self.root / "minimal-bin"
        minimal_bin.mkdir()
        for command in ("bash", "git", "dirname"):
            (minimal_bin / command).symlink_to(shutil.which(command))
        result = self.run_setup(PATH=str(minimal_bin))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("uv tool install pre-commit", result.stderr)

    def test_launcher_does_not_start_agent_after_failed_setup(self):
        result = subprocess.run(
            [str(self.scripts / "agent-session.sh"), "sh", "-c", "echo agent-started"],
            cwd=self.root,
            env=self.env | {"FAIL_SETUP": "1"},
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("agent-started", result.stdout)

    def test_launcher_preserves_arguments_and_exit_status(self):
        result = subprocess.run(
            [
                str(self.scripts / "agent-session.sh"),
                "sh",
                "-c",
                'printf "%s" "$1"; exit 17',
                "agent",
                "argument with spaces",
            ],
            cwd=self.root,
            env=self.env,
            text=True,
            capture_output=True,
        )
        self.assertEqual(result.returncode, 17, result.stderr)
        self.assertTrue(result.stdout.endswith("argument with spaces"))


if __name__ == "__main__":
    unittest.main()
