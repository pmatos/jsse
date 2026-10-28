"""Run an oracle subprocess tree in its own session and never leave it behind.

`run_group` starts the command as the leader of a new session (so a process group of its own), and in every
exit path (normal return, timeout, KeyboardInterrupt, SIGTERM/SIGHUP, interpreter exit) kills the whole group:
SIGTERM, a short grace, then SIGKILL. Forked workers (mocha, hereby runtests-parallel) that outlive their
host otherwise get re-parented to the init/user manager and keep running. Output goes to temporary files, so a
straggler that inherited the pipes cannot block the reader.
"""

import atexit
import os
import signal
import subprocess
import tempfile
import threading
import time

GRACE = 2.0
_lock = threading.Lock()
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


def reap_group(child, grace=GRACE):
    """Kill every process of `child`'s group, wait for the group to vanish, and reap the leader."""
    pgid = child.pid
    _signal_group(pgid, signal.SIGTERM)
    deadline = time.monotonic() + grace
    while time.monotonic() < deadline:
        if child.poll() is not None and not _alive(pgid):
            return
        time.sleep(0.02)
    _signal_group(pgid, signal.SIGKILL)
    child.wait()
    deadline = time.monotonic() + grace
    while _alive(pgid) and time.monotonic() < deadline:
        time.sleep(0.02)


def kill_all():
    """Stop launching and kill the group of every live run (atexit and signal handlers)."""
    global _stopping
    with _lock:
        _stopping = True
        children = list(_live.values())
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


def run_group(cmd, cwd, env, timeout):
    """Run `cmd`; return (exit code as str or "timeout", stdout bytes, stderr bytes)."""
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        with _lock:
            if _stopping:
                raise RuntimeError("oracle is shutting down")
            child = subprocess.Popen(
                cmd,
                cwd=cwd,
                env=env,
                stdin=subprocess.DEVNULL,
                stdout=out,
                stderr=err,
                start_new_session=True,
            )
            _live[child.pid] = child
        try:
            try:
                child.wait(timeout=timeout)
                code = str(child.returncode)
            except subprocess.TimeoutExpired:
                code = "timeout"
        finally:
            reap_group(child)
            with _lock:
                _live.pop(child.pid, None)
        out.seek(0)
        err.seek(0)
        return code, out.read(), err.read()
