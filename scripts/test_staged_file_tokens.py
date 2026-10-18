import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
HOOK = REPO_ROOT / "scripts" / "check_staged_file_tokens.py"


class StagedFileTokenTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.env = os.environ.copy()
        for key in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR"):
            self.env.pop(key, None)
        self.git("init", "--quiet")
        self.git("config", "user.name", "Hook Test")
        self.git("config", "user.email", "hook-test@example.invalid")
        self.git("config", "core.hooksPath", "/dev/null")
        self.git("config", "commit.gpgsign", "false")
        self.write(".gitattributes", (REPO_ROOT / ".gitattributes").read_bytes())
        self.git("add", "--", ".gitattributes")

    def git(self, *args: str) -> bytes:
        return subprocess.run(
            ["git", *args],
            cwd=self.root,
            env=self.env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=True,
        ).stdout

    def write(self, name: str, data: bytes):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)

    def stage(self, name: str, data: bytes):
        self.write(name, data)
        self.git("add", "--", name)

    def run_hook(self) -> subprocess.CompletedProcess:
        return subprocess.run(
            [sys.executable, str(HOOK)],
            cwd=self.root,
            env=self.env,
            text=True,
            capture_output=True,
            check=False,
        )

    def test_empty_index_passes(self):
        self.git("read-tree", "--empty")
        self.assertEqual(self.run_hook().returncode, 0)

    def test_exact_boundary_passes_but_one_more_byte_fails(self):
        self.stage("module.rs", b"x" * 20_000)
        self.assertEqual(self.run_hook().returncode, 0)
        self.stage("module.rs", b"x" * 20_001)
        result = self.run_hook()
        self.assertEqual(result.returncode, 1)
        for message in (
            "module.rs",
            "20,001 bytes",
            "5,000.25 tokens",
            "child modules",
            "update imports",
            "git add",
        ):
            self.assertIn(message, result.stderr)

    def test_counts_utf8_bytes(self):
        self.stage("unicode.md", "é".encode() * 10_001)
        result = self.run_hook()
        self.assertEqual(result.returncode, 1)
        self.assertIn("20,002 bytes", result.stderr)

    def test_staged_large_file_cannot_be_hidden_by_unstaged_shrink(self):
        self.stage("module.rs", b"x" * 20_001)
        self.write("module.rs", b"small")
        self.assertEqual(self.run_hook().returncode, 1)

    def test_unstaged_growth_and_untracked_files_do_not_fail(self):
        self.stage("module.rs", b"small")
        self.write("module.rs", b"x" * 20_001)
        self.write("untracked.rs", b"x" * 20_001)
        self.assertEqual(self.run_hook().returncode, 0)

    def test_unchanged_large_file_is_not_checked(self):
        self.stage("old.rs", b"x" * 20_001)
        self.git("commit", "--quiet", "-m", "baseline")
        self.stage("new.rs", b"small")
        self.assertEqual(self.run_hook().returncode, 0)

    def test_modifying_existing_large_file_fails(self):
        self.stage("old.rs", b"x" * 20_001)
        self.git("commit", "--quiet", "-m", "baseline")
        self.stage("old.rs", b"y" * 20_001)
        self.assertEqual(self.run_hook().returncode, 1)

    def test_deletion_passes_and_rename_checks_destination(self):
        self.stage("old.rs", b"x" * 20_001)
        self.git("commit", "--quiet", "-m", "baseline")
        self.git("mv", "--", "old.rs", "new.rs")
        result = self.run_hook()
        self.assertEqual(result.returncode, 1)
        self.assertIn("new.rs", result.stderr)
        self.assertNotIn("old.rs", result.stderr)
        self.git("commit", "--quiet", "-m", "rename")
        self.git("rm", "--", "new.rs")
        self.assertEqual(self.run_hook().returncode, 0)

    def test_generated_outputs_and_lockfiles_are_exempt(self):
        for name in (
            "Cargo.lock",
            "fuzz/Cargo.lock",
            "package-lock.json",
            "npm-shrinkwrap.json",
            "src/unicode_tables.rs",
            "src/emoji_strings.rs",
            "CHANGELOG.md",
            "test262-pass.txt",
            "test262-pass-node.txt",
            "benchmarks/results.csv",
            "scripts/.unicode_cache/source.txt",
        ):
            self.stage(name, b"x" * 20_001)
        self.assertEqual(self.run_hook().returncode, 0)
        # Generator implementations themselves are authored code.
        self.stage("scripts/generate_unicode_tables.py", b"x" * 20_001)
        self.assertEqual(self.run_hook().returncode, 1)

    def test_unstaged_exemption_cannot_bypass_limit(self):
        self.stage("large.rs", b"x" * 20_001)
        self.write(".gitattributes", b"large.rs token-limit-ignore\n")
        self.assertEqual(self.run_hook().returncode, 1)
        self.git("add", "--", ".gitattributes")
        self.write(".gitattributes", b"")
        self.assertEqual(self.run_hook().returncode, 0)

    def test_binary_assets_symlinks_and_gitlinks_pass(self):
        self.stage("asset.png", b"\0" + b"x" * 20_001)
        self.git("commit", "--quiet", "-m", "baseline")
        (self.root / "link").symlink_to("missing.rs")
        self.git("add", "--", "link")
        oid = self.git("rev-parse", "HEAD").decode().strip()
        self.git("update-index", "--add", "--cacheinfo", f"160000,{oid},upstream")
        self.stage("asset.png", b"\0" + b"y" * 20_001)
        self.assertEqual(self.run_hook().returncode, 0)

    def test_unusual_filenames_and_multiple_violations(self):
        for name in ("space name.rs", "tab\tname.rs", "newline\nname.rs", "-dash.rs"):
            self.stage(name, b"x" * 20_001)
        result = self.run_hook()
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stderr.count("20,001 bytes"), 4)
        self.assertIn(repr("newline\nname.rs"), result.stderr)


if __name__ == "__main__":
    unittest.main()
