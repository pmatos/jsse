"""Run an oracle subprocess tree in its own session and never leave it behind.

`group` (a context manager; `run_group`, `run_measured` and `call` are built on it) starts the command as the
leader of a new session (so a process group of its own), and in every exit path (normal return, timeout, KeyboardInterrupt, SIGTERM/SIGHUP, interpreter exit) kills the whole group:
SIGTERM, a short grace, then SIGKILL. Forked workers (mocha, hereby runtests-parallel) that outlive their
host otherwise get re-parented to the init/user manager and keep running. Output goes to temporary files, so a
straggler that inherited the pipes cannot block the reader.
"""

import atexit
import contextlib
import os
import signal
import subprocess
import sys
import tempfile
import threading
import time

GRACE = 2.0
_lock = threading.RLock()
_live = {}
_stopping = False
_installed = False


def _alive(pgid):
    try:
        os.killpg(pgid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def _signal_group(pgid, sig):
    try:
        os.killpg(pgid, sig)
    except (ProcessLookupError, PermissionError):
        pass


def _gone(child):
    return child.poll() is not None and not _alive(child.pid)


def reap_group(child, grace=GRACE):
    """Kill every process of `child`'s group, wait for the group to vanish, and reap the leader."""
    pgid = child.pid
    _signal_group(pgid, signal.SIGTERM)
    deadline = time.monotonic() + grace
    while time.monotonic() < deadline:
        if _gone(child):
            return
        time.sleep(0.02)
    _signal_group(pgid, signal.SIGKILL)
    child.wait()
    deadline = time.monotonic() + grace
    while _alive(pgid) and time.monotonic() < deadline:
        time.sleep(0.02)


def kill_all():
    """Stop launching and kill the group of every live run (atexit and signal handlers).

    SIGTERM first and a grace of twice reap_group's (so a nested driver finishes its own pass), then SIGKILL: a
    group leader that is itself an oracle driver runs its own cleanup on SIGTERM and so takes the groups it
    started with it; SIGKILL alone would orphan them.
    """
    global _stopping
    with _lock:
        _stopping = True
        children = list(_live.values())
    for child in children:
        _signal_group(child.pid, signal.SIGTERM)
    deadline = time.monotonic() + 2 * GRACE
    while time.monotonic() < deadline and not all(_gone(c) for c in children):
        time.sleep(0.02)
    for child in children:
        _signal_group(child.pid, signal.SIGKILL)


def _on_signal(signum, _frame):
    kill_all()
    raise SystemExit(128 + signum)


def install_cleanup():
    """Kill live groups at interpreter exit and on SIGINT/SIGTERM/SIGHUP (main thread only)."""
    global _installed
    if _installed:
        return
    _installed = True
    atexit.register(kill_all)
    for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, _on_signal)


@contextlib.contextmanager
def group(cmd, **popen_kw):
    """Start `cmd` as the leader of its own session, registered for cleanup; yield the Popen.

    On leaving the block (normally, by an exception, or by the interrupt handlers) the whole group is killed
    and the leader reaped. Streams are as for Popen (inherited unless given).
    """
    with _lock:
        if _stopping:
            raise RuntimeError("oracle is shutting down")
        child = subprocess.Popen(cmd, start_new_session=True, **popen_kw)
        _live[child.pid] = child
    try:
        yield child
    finally:
        reap_group(child)
        with _lock:
            _live.pop(child.pid, None)


def run_group(cmd, cwd, env, timeout):
    """Run `cmd`; return (exit code as str or "timeout", stdout bytes, stderr bytes)."""
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        with group(
            cmd,
            cwd=cwd,
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=out,
            stderr=err,
        ) as child:
            try:
                child.wait(timeout=timeout)
                code = str(child.returncode)
            except subprocess.TimeoutExpired:
                code = "timeout"
        out.seek(0)
        err.seek(0)
        return code, out.read(), err.read()


def call(cmd, **popen_kw):
    """Run `cmd` in its own group with the given (default inherited) output and no timeout; return its status."""
    with group(cmd, **popen_kw) as child:
        return child.wait()


def run_measured(cmd, cwd, env, timeout, merge_stderr=False):
    """Like run_group, plus wall seconds and the peak RSS in MiB of the largest process of the tree.

    Returns (exit code as int or "timeout", stdout, stderr, seconds, MiB); stderr is b"" when merged into
    stdout. The peak is the `ru_maxrss` that wait4 reports for the child and every descendant it reaped.
    """
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        stderr = subprocess.STDOUT if merge_stderr else err
        with group(
            cmd,
            cwd=cwd,
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=out,
            stderr=stderr,
        ) as proc:
            start = time.monotonic()
            code = "timeout"
            while True:
                pid, status, usage = os.wait4(proc.pid, os.WNOHANG)
                if pid:
                    code = os.waitstatus_to_exitcode(status)
                    break
                if time.monotonic() - start > timeout:
                    os.killpg(proc.pid, signal.SIGKILL)
                    _, _, usage = os.wait4(proc.pid, 0)
                    break
                time.sleep(0.05)
            secs = time.monotonic() - start
            proc.returncode = 0
        out.seek(0)
        err.seek(0)
        rss = usage.ru_maxrss / (1024 * 1024 if sys.platform == "darwin" else 1024)
        return code, out.read(), err.read(), secs, rss
