import importlib.util
import os
import signal
import stat
import subprocess
import sys
import tempfile
import textwrap
import time
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
RUNNER = REPO_ROOT / "scripts" / "run-test262.py"


def _load_runner():
    spec = importlib.util.spec_from_file_location("run_test262", RUNNER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


runner = _load_runner()
PREFIX = runner.SCRATCH_PREFIX


def frontmatter(*lines: str) -> str:
    body = "\n".join(lines)
    return f"/*---\n{body}\n---*/\n"


class RunTest262ExitStatusTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.test_file = self.write_file(
            "test262/test/sample.js", frontmatter("flags: [raw]")
        )

    def tearDown(self):
        self.tmp.cleanup()

    def write_file(self, relpath: str, content: str = "") -> Path:
        path = self.root / relpath
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
        return path

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
        self.write_file("test262-extra/module-test.mjs")

        result = self.run_runner(
            self.write_engine(0), paths=("test262-extra/module-test.mjs",)
        )

        self.assertEqual(result.returncode, 2)
        self.assertIn("non-fixture .mjs files would not be collected", result.stderr)
        self.assertIn("test262-extra/module-test.mjs", result.stderr)
        self.assertIn("flags: [module]", result.stderr)

    def test_directory_with_non_fixture_mjs_is_rejected(self):
        self.write_file("test262-extra/nested/module-test.mjs")

        result = self.run_runner(self.write_engine(0), paths=("test262-extra",))

        self.assertEqual(result.returncode, 2)
        self.assertIn("test262-extra/nested/module-test.mjs", result.stderr)

    def test_directory_allows_mjs_fixtures(self):
        self.write_file("test262-extra/module-test.js", frontmatter("flags: [module]"))
        self.write_file("test262-extra/dep_FIXTURE.mjs")

        result = self.run_runner(
            self.write_engine(0),
            "--fail-on-failures",
            paths=("test262-extra",),
        )

        self.assertEqual(result.returncode, 0)
        self.assertIn("Files:   1", result.stdout)

    def test_test262_submodule_mjs_does_not_block_collection(self):
        self.write_file("test262/tools/generator.mjs")

        result = self.run_runner(
            self.write_engine(0), "--fail-on-failures", paths=("test262",)
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("would not be collected", result.stderr)


class ScratchFileTests(unittest.TestCase):
    """Scratch files are written next to the test they wrap, inside the
    read-only test262 checkout, so a run that dies without cleaning up leaves
    them among the tests."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        # find_tests only walks the corpus subdirectories, so the fixture has
        # to live in one of them for the collection test to be meaningful.
        self.test_dir = self.root / "test262" / "test" / "language"
        self.test_dir.mkdir(parents=True)
        (self.test_dir / "sample.js").write_text(
            frontmatter("flags: [raw]"), encoding="utf-8"
        )

    def tearDown(self):
        self.tmp.cleanup()

    def write_scratch(self, name: str, age_s: float = 0.0) -> Path:
        path = self.test_dir / name
        path.write_text("// leaked scratch file\n", encoding="utf-8")
        if age_s:
            stamp = time.time() - age_s
            os.utime(path, (stamp, stamp))
        return path

    def test_scratch_names_are_recognised(self):
        self.assertTrue(runner._is_scratch(Path(f"{PREFIX}a0o8myw6.js")))
        self.assertTrue(runner._is_scratch(Path(f"{PREFIX}_g3ol4p_.js")))

    def test_created_scratch_files_match_the_sweep_pattern(self):
        # Guards against the prefix and the regex drifting apart: a name the
        # runner actually produces must be one the sweep recognises.
        with tempfile.NamedTemporaryFile(
            prefix=runner.SCRATCH_PREFIX, suffix=".js", dir=self.test_dir
        ) as tmp:
            created = Path(tmp.name)
            self.assertTrue(runner._is_scratch(created))
            self.assertIn(created, set(self.test_dir.rglob(f"{PREFIX}*.js")))

    def test_real_tests_are_not_mistaken_for_scratch(self):
        # Only the exact shape the runner emits (prefix + 8 chars + .js) may be
        # swept; near misses must survive.
        for name in (
            f"{PREFIX}.js",
            f"{PREFIX}short.js",
            f"{PREFIX}toolonganame.js",
            f"{PREFIX}ABCDEFGH.js",
            f"temporal-{PREFIX}a0o8myw6.js",
            f"{PREFIX}a0o8myw6.mjs",
        ):
            with self.subTest(name=name):
                self.assertFalse(runner._is_scratch(Path(name)))

    def test_generic_tempfile_names_are_not_claimed(self):
        # A bare `tempfile` name proves nothing about who wrote it, so it is
        # neither deleted nor skipped: both would act on a file we cannot claim.
        for name in ("tmpa0o8myw6.js", "tmp_g3ol4p_.js"):
            with self.subTest(name=name):
                self.assertFalse(runner._is_scratch(Path(name)))

    def test_scratch_files_are_not_collected_as_tests(self):
        self.write_scratch(f"{PREFIX}a0o8myw6.js")

        tests = runner.find_tests(self.root / "test262", None)

        self.assertEqual([p.name for p in tests], ["sample.js"])

    def test_legacy_shaped_files_are_still_collected(self):
        # A real test may legitimately be named like a bare `tempfile`. Skipping
        # it would silently shrink the run, corrupting the same scenario total
        # the scratch filter exists to protect, so only prefixed names are ours.
        self.write_scratch("tmpa0o8myw6.js")

        tests = runner.find_tests(self.root / "test262", None)

        self.assertEqual([p.name for p in tests], ["sample.js", "tmpa0o8myw6.js"])

    def test_sweep_removes_stale_scratch_files(self):
        stale = self.write_scratch(f"{PREFIX}a0o8myw6.js", age_s=10_000)

        removed = runner.sweep_scratch_files([self.test_dir], min_age_s=300)

        self.assertEqual(removed, 1)
        self.assertFalse(stale.exists())

    def test_sweep_spares_stale_generic_tempfile_names(self):
        # The whole point of the prefix: an unrelated tool's `tempfile`, or a
        # real test shaped like one, is not ours to unlink.
        foreign = self.write_scratch("tmpa0o8myw6.js", age_s=10_000)

        removed = runner.sweep_scratch_files([self.test_dir], min_age_s=300)

        self.assertEqual(removed, 0)
        self.assertTrue(foreign.exists())

    def test_sweep_spares_scratch_files_of_a_concurrent_run(self):
        live = self.write_scratch(f"{PREFIX}a0o8myw6.js")

        removed = runner.sweep_scratch_files([self.test_dir], min_age_s=300)

        self.assertEqual(removed, 0)
        self.assertTrue(live.exists())

    def test_explicitly_named_scratch_file_is_not_collected(self):
        scratch = self.write_scratch(f"{PREFIX}a0o8myw6.js")

        tests = runner.find_tests(self.root / "test262", [str(scratch)])

        self.assertEqual(tests, [])

    def test_explicitly_named_legacy_shaped_file_is_still_collected(self):
        legacy = self.write_scratch("tmpa0o8myw6.js")

        tests = runner.find_tests(self.root / "test262", [str(legacy)])

        self.assertEqual(tests, [legacy])

    def test_glob_expanded_selection_drops_only_the_scratch_file(self):
        # `run-test262.py test262/test/language/*.js` reaches find_tests as a
        # list of explicit file paths, one of which may be a leaked scratch file.
        sample = self.test_dir / "sample.js"
        scratch = self.write_scratch(f"{PREFIX}a0o8myw6.js")

        tests = runner.find_tests(self.root / "test262", [str(sample), str(scratch)])

        self.assertEqual([p.name for p in tests], ["sample.js"])

    def test_sweep_leaves_real_tests_alone(self):
        sample = self.test_dir / "sample.js"
        decoy = self.write_scratch(f"{PREFIX}short.js", age_s=10_000)

        runner.sweep_scratch_files([self.test_dir], min_age_s=300)

        self.assertTrue(sample.exists())
        self.assertTrue(decoy.exists())


class WorkerCleanupTests(unittest.TestCase):
    def test_signal_during_the_write_still_unlinks_the_scratch_file(self):
        """The scratch file exists on disk from creation, not from write.

        So registration cannot wait until the write finishes: `combined` can be
        hundreds of KB, and a signal landing inside that window would otherwise
        find no path to unlink and leave the file behind in the submodule.
        """
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            test_dir = root / "test262" / "test" / "language"
            test_dir.mkdir(parents=True)
            test_file = test_dir / "sample.js"
            test_file.write_text(frontmatter("flags: [raw]"), encoding="utf-8")

            # Fire SIGTERM from inside `write`, i.e. after the file exists but
            # before anything has been written to it.
            code = textwrap.dedent(
                f"""\
                import os, runpy, signal, tempfile
                runner = runpy.run_path({str(RUNNER)!r})
                signal.signal(signal.SIGTERM, runner["_worker_cleanup_handler"])

                real_ntf = tempfile.NamedTemporaryFile

                def spy(*a, **kw):
                    handle = real_ntf(*a, **kw)
                    real_write = handle.write

                    def write(data):
                        os.kill(os.getpid(), signal.SIGTERM)
                        return real_write(data)

                    handle.write = write
                    return handle

                tempfile.NamedTemporaryFile = spy
                runner["run_single_test"]((
                    "sample.js",
                    {str(test_file)!r},
                    "default",
                    5,
                    {str(root / "test262")!r},
                    "jsse",
                    "/nonexistent/jsse-binary",
                    False,
                ))
                """
            )
            result = subprocess.run(
                [sys.executable, "-c", code], capture_output=True, text=True
            )

            self.assertEqual(result.returncode, 128 + signal.SIGTERM, result.stderr)
            leaked = sorted(p.name for p in test_dir.iterdir() if p != test_file)
            self.assertEqual(leaked, [], f"scratch file left behind: {leaked}")

    def test_sigterm_handler_unlinks_the_in_flight_scratch_file(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            scratch = Path(tmpdir) / f"{PREFIX}a0o8myw6.js"
            scratch.write_text("// in flight\n", encoding="utf-8")

            # The handler exits the process, so run it in a child.
            code = textwrap.dedent(
                f"""\
                import os, runpy, signal, sys
                runner = runpy.run_path({str(RUNNER)!r})
                runner["_active_scratch_path"] = {str(scratch)!r}
                # Rebind the module global the handler closes over.
                handler = runner["_worker_cleanup_handler"]
                handler.__globals__["_active_scratch_path"] = {str(scratch)!r}
                handler(signal.SIGTERM, None)
                """
            )
            result = subprocess.run(
                [sys.executable, "-c", code], capture_output=True, text=True
            )

            self.assertEqual(result.returncode, 128 + signal.SIGTERM)
            self.assertFalse(
                scratch.exists(), "handler should unlink the in-flight scratch file"
            )


if __name__ == "__main__":
    unittest.main()
