"""Issue #1425: every TypeScript oracle driver runs its engine through scripts/oracle_procs.py, so an interrupted,
timed-out or finished driver leaves no process of the tree behind.

Each driver's own launch function runs a stub engine that starts a long-lived grandchild (the shape of mocha's
forked workers) and records its pid. The scenarios: the run times out, the engine exits leaving the grandchild, and
the driver gets SIGINT/SIGTERM mid-run. In every one the grandchild must be gone.
"""

import glob
import importlib.util
import os
import signal
import stat
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS))
import oracle_procs  # noqa: E402

STUB = """#!/bin/sh
sleep 300 &
echo $! > "{pidfile}.tmp"
mv "{pidfile}.tmp" "{pidfile}"
{tail}
"""

DRIVER = """
import importlib.util, os, sys
sys.path.insert(0, {scripts!r})
spec = importlib.util.spec_from_file_location("driver_under_test", {path!r})
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
import oracle_procs
oracle_procs.install_cleanup()
STUB, TMP, TIMEOUT = {stub!r}, {tmp!r}, {timeout}
{call}
"""

# name, script, statement run with the module `m`, STUB, TMP and TIMEOUT in scope, whether the stub's grandchild may
# outlive a normal exit (a driver that reads the engine's stdout to EOF would wait for it), whether TIMEOUT applies
DRIVERS = [
    (
        "hereby",
        "run-hereby-oracle.py",
        'm.execute("node", STUB, TMP, [], {}, 0, TIMEOUT, TMP, TMP)',
        True,
        True,
    ),
    (
        "hereby-local",
        "run-hereby-local-oracle.py",
        'm.hereby_local("node", STUB, TMP, TIMEOUT)',
        True,
        True,
    ),
    (
        "tsc-projects",
        "run-tsc-projects-oracle.py",
        'm.run_case("node", STUB, TMP, TMP, ("c", "app", "0", [["x"]], None), TIMEOUT)',
        True,
        True,
    ),
    (
        "tsc-watch",
        "run-tsc-watch-oracle.py",
        'm.run_case("node", STUB, TMP, TMP, ("w", "watch", ["-w", "-p", "."], [["@wait"]]),'
        " TIMEOUT, 0.1, m.FIX)",
        True,
        True,
    ),
    (
        "tsc-selfcompile",
        "run-tsc-selfcompile-oracle.py",
        "m.run_measured([STUB], TMP, dict(os.environ), TIMEOUT)",
        True,
        True,
    ),
    ("ts-suite", "run-ts-suite-oracle.py", "m.sh([STUB], TMP)", True, False),
    (
        "ts-unittests-config-sys",
        "run-ts-unittests-config-sys-oracle.py",
        "m.launch([STUB], TMP, dict(os.environ), TIMEOUT)",
        True,
        True,
    ),
    (
        "ts-compiler-df",
        "run-ts-compiler-df-oracle.py",
        "m.BASE = [STUB]; m.run([])",
        False,
        False,
    ),
]


def gone(pid, wait=5.0):
    end = time.monotonic() + wait
    while time.monotonic() < end:
        try:
            with open(f"/proc/{pid}/stat") as f:
                if f.read().rsplit(")", 1)[1].split()[0] == "Z":
                    return True
        except (FileNotFoundError, ProcessLookupError):
            return True
        time.sleep(0.05)
    return False


class Case(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="oracle-procs-1425-")
        self.pidfile = os.path.join(self.tmp, "pid")
        self.addCleanup(subprocess.call, ["rm", "-rf", self.tmp])
        self.addCleanup(self.reap)

    def reap(self):
        try:
            with open(self.pidfile) as f:
                os.kill(int(f.read()), signal.SIGKILL)
        except (OSError, ValueError):
            pass

    def stub(self, tail):
        path = os.path.join(self.tmp, "engine")
        with open(path, "w") as f:
            f.write(STUB.format(pidfile=self.pidfile, tail=tail))
        os.chmod(path, os.stat(path).st_mode | stat.S_IXUSR)
        return path

    def grandchild(self):
        end = time.monotonic() + 20
        while not os.path.exists(self.pidfile) and time.monotonic() < end:
            time.sleep(0.02)
        with open(self.pidfile) as f:
            return int(f.read())

    def driver(self, script, call, tail, timeout):
        self.reap()
        if os.path.exists(self.pidfile):
            os.remove(self.pidfile)
        work = tempfile.mkdtemp(dir=self.tmp)
        code = DRIVER.format(
            scripts=str(SCRIPTS),
            path=str(SCRIPTS / script),
            stub=self.stub(tail),
            tmp=work,
            timeout=timeout,
            call=call,
        )
        return subprocess.Popen(
            [sys.executable, "-c", code],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
        )

    def finish(self, proc):
        self.addCleanup(lambda: proc.poll() is None and proc.kill())
        return proc.wait(60)


class DriverTest(Case):
    def test_a_timeout_kills_the_engines_children(self):
        for name, script, call, _, has_timeout in DRIVERS:
            if not has_timeout:
                continue
            with self.subTest(name):
                proc = self.driver(script, call, "wait", 1)
                self.assertEqual(self.finish(proc), 0)
                self.assertTrue(gone(self.grandchild()))

    def test_a_finished_engine_leaves_no_straggler(self):
        for name, script, call, may_straggle, _ in DRIVERS:
            if not may_straggle:
                continue
            with self.subTest(name):
                proc = self.driver(script, call, "exit 0", 30)
                self.assertEqual(self.finish(proc), 0)
                self.assertTrue(gone(self.grandchild()))

    def test_an_interrupted_driver_leaves_no_child_process(self):
        for sig in (signal.SIGINT, signal.SIGTERM):
            for name, script, call, _, _ in DRIVERS:
                with self.subTest(driver=name, signal=sig.name):
                    proc = self.driver(script, call, "wait", 120)
                    pid = self.grandchild()
                    time.sleep(0.2)
                    os.kill(proc.pid, sig)
                    self.assertEqual(self.finish(proc), 128 + sig)
                    self.assertTrue(gone(pid))


class NestedTest(Case):
    def test_an_interrupted_driver_takes_the_oracle_it_runs_with_it(self):
        inner = os.path.join(self.tmp, "inner.py")
        with open(inner, "w") as f:
            f.write(
                "import sys\nsys.path.insert(0, %r)\nimport oracle_procs as p\n"
                "p.install_cleanup()\np.run_group([%r], None, None, 120)\n"
                % (str(SCRIPTS), self.stub("wait"))
            )
        outer = (
            "import sys\nsys.path.insert(0, %r)\nimport oracle_procs as p\n"
            "p.install_cleanup()\np.run_group([sys.executable, %r], None, None, 120)\n"
            % (str(SCRIPTS), inner)
        )
        proc = subprocess.Popen([sys.executable, "-c", outer], start_new_session=True)
        pid = self.grandchild()
        time.sleep(0.2)
        proc.terminate()
        self.assertEqual(self.finish(proc), 143)
        self.assertTrue(gone(pid))


class SharedHelpersTest(unittest.TestCase):
    def test_call_returns_the_status_and_forwards_streams(self):
        with tempfile.TemporaryFile() as out:
            code = oracle_procs.call(
                [sys.executable, "-c", "print('hi'); raise SystemExit(3)"], stdout=out
            )
            out.seek(0)
            self.assertEqual((code, out.read()), (3, b"hi\n"))

    def test_run_measured_can_merge_stderr_into_stdout(self):
        code, out, err, _, rss = oracle_procs.run_measured(
            [
                sys.executable,
                "-c",
                "import sys; print('o'); print('e', file=sys.stderr)",
            ],
            None,
            None,
            30,
            merge_stderr=True,
        )
        self.assertEqual((code, err), (0, b""))
        self.assertEqual(sorted(out.split()), [b"e", b"o"])
        self.assertGreater(rss, 1)


class ReentrancyTest(unittest.TestCase):
    def test_a_signal_while_the_registry_is_locked_does_not_deadlock(self):
        code = (
            "import sys; sys.path.insert(0, %r); import oracle_procs as p\n"
            "with p._lock:\n    p.kill_all()\nprint('ok')\n" % str(SCRIPTS)
        )
        r = subprocess.run(
            [sys.executable, "-c", code], capture_output=True, text=True, timeout=30
        )
        self.assertEqual(r.stdout.strip(), "ok")


class AdoptionTest(unittest.TestCase):
    """No driver starts an engine behind oracle_procs' back, and each one installs the interrupt cleanup."""

    def test_no_driver_spawns_a_process_by_hand(self):
        scripts = sorted(
            glob.glob(str(SCRIPTS / "run-ts-*-oracle.py"))
            + glob.glob(str(SCRIPTS / "run-tsc-*-oracle.py"))
            + glob.glob(str(SCRIPTS / "run-hereby*-oracle.py"))
        )
        self.assertGreater(len(scripts), 10)
        for path in scripts:
            with self.subTest(os.path.basename(path)):
                self.assertNotIn("subprocess.Popen", Path(path).read_text())

    def test_every_listed_driver_installs_the_cleanup_in_main(self):
        for name, script, *_ in DRIVERS:
            with self.subTest(name):
                text = (SCRIPTS / script).read_text()
                self.assertIn("oracle_procs.install_cleanup()", text)

    def test_importing_a_driver_has_no_side_effects(self):
        for _, script, *_ in DRIVERS:
            with self.subTest(script):
                spec = importlib.util.spec_from_file_location("d", SCRIPTS / script)
                module = importlib.util.module_from_spec(spec)
                before = signal.getsignal(signal.SIGTERM)
                spec.loader.exec_module(module)
                self.assertIs(signal.getsignal(signal.SIGTERM), before)


if __name__ == "__main__":
    unittest.main()
