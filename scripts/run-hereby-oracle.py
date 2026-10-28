#!/usr/bin/env python3
"""Run hereby and its dependencies on Node and on jsse --node and diff the results (issue #1251).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-hereby-oracle.py [options] [case ...]

TS_SRC_DIR is a TypeScript v6.0.3 checkout with `npm ci` done (node_modules/hereby and its
dependencies); without it the script prints SKIP and exits 0 unless --require is given. The
cases run the hereby CLI against the TypeScript Herebyfile (--tasks, --help, task errors) and
against the fixtures in scripts/hereby-fixtures (task graphs, failing tasks, Herebyfile load
errors, colour, a pseudo-terminal) and exercise chokidar, glob, picocolors, @esfx/canceltoken,
which and minimist (deps/deps.mjs). Stdout, stderr and the exit code must be identical after
normalising paths, durations, pids and Node's own `node:internal` frames (see normalise()).
Never runs a build task. Exits 1 on any difference.

Options: --jobs N (default 4), --timeout SECONDS (default 300), --ci, --list, --require,
--jsse PATH (else $JSSE, else target/release/jsse).
"""

import argparse
import difflib
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIX = os.path.join(ROOT, "scripts", "hereby-fixtures")
PTY = os.path.join(ROOT, "scripts", "tty-fixtures", "pty-run.py")
DROP_ENV = ("NODE_OPTIONS", "FORCE_COLOR", "NO_COLOR", "COLUMNS", "NODE_DISABLE_COLORS")
DURATION = re.compile(r"(?<= in )\d[\d.]*(?:ms|s|m|h|d)(?: \d[\d.]*(?:ms|s|m|h|d))*")
PID = re.compile(r"\(node:\d+\)")
FRAME = re.compile(r"^\s+at ")

# name, directory under hereby-fixtures ("" = the TypeScript checkout), argv, env, pty columns
HEREBY_CASES = [
    ("ts-tasks", "", ["--tasks"], {}, 0),
    ("ts-tasks-simple", "", ["--tasks-simple"], {}, 0),
    ("ts-tasks-colour", "", ["--tasks"], {"FORCE_COLOR": "1"}, 0),
    ("ts-tasks-pty-narrow", "", ["--tasks"], {}, 50),
    ("ts-tasks-pty-wide", "", ["--tasks"], {}, 140),
    ("ts-help", "", ["--help"], {}, 0),
    ("ts-version", "", ["--version"], {}, 0),
    ("ts-missing-task", "", ["no-such-task"], {}, 0),
    ("ts-typo-task", "", ["build-sr"], {}, 0),
    (
        "ts-missing-herebyfile",
        "",
        ["--herebyfile", "<WORK>/absent/Herebyfile.mjs"],
        {},
        0,
    ),
    ("tasks-list", "tasks", ["--tasks"], {}, 0),
    ("tasks-default", "tasks", [], {}, 0),
    ("tasks-chain", "tasks", ["build", "group"], {}, 0),
    ("tasks-colour", "tasks", ["build"], {"FORCE_COLOR": "1"}, 0),
    ("tasks-no-colour", "tasks", ["build"], {"NO_COLOR": "1"}, 0),
    (
        "tasks-both-colour-env",
        "tasks",
        ["build"],
        {"FORCE_COLOR": "1", "NO_COLOR": "1"},
        0,
    ),
    ("tasks-pty", "tasks", ["build"], {}, 100),
    ("tasks-fail-async", "tasks", ["fail-async"], {}, 0),
    ("tasks-fail-sync", "tasks", ["fail-sync"], {}, 0),
    ("tasks-fail-string", "tasks", ["fail-string"], {}, 0),
    ("tasks-fail-both", "tasks", ["fail-both"], {}, 0),
    ("tasks-fail-pty", "tasks", ["fail-async"], {}, 100),
    ("tasks-missing", "tasks", ["fail-asyn"], {}, 0),
    ("cycle", "cycle", [], {}, 0),
    ("duplicate-name", "dup", [], {}, 0),
    ("no-exports", "noexport", [], {}, 0),
    ("load-throws", "loadthrow", [], {}, 0),
    ("no-herebyfile", "empty", [], {}, 0),
]
DEPS_CASES = [
    ("deps", {}),
    ("deps-colour", {"FORCE_COLOR": "1"}),
    ("deps-no-colour", {"NO_COLOR": "1"}),
    ("deps-dumb-term", {"TERM": "dumb"}),
]


def normalise(text, work, ts_dir):
    """Make two runs comparable: paths, durations, pids, and Node's internal stack frames.

    Frames inside node:internal are dropped on both sides; every user frame, module top-level
    and `at async` ones included, must match (issue #1368).
    """
    text = text.replace(work, "<WORK>").replace(ts_dir, "<TS>")
    text = DURATION.sub("<T>", text)
    text = PID.sub("(node:<PID>)", text)
    kept = [
        line
        for line in text.splitlines()
        if not (FRAME.match(line) and "node:internal" in line)
    ]
    return "\n".join(kept) + "\n"


def prepare(work, engine, name, fixture, ts_dir):
    cwd = os.path.join(work, engine, name)
    if fixture:
        src = os.path.join(FIX, fixture)
        os.makedirs(os.path.dirname(cwd), exist_ok=True)
        if os.path.isdir(src):
            shutil.copytree(src, cwd)
        else:
            os.makedirs(cwd)
        os.symlink(
            os.path.join(ts_dir, "node_modules"), os.path.join(cwd, "node_modules")
        )
    else:
        cwd = ts_dir
    return cwd


def execute(engine, exe, cwd, argv, env_extra, cols, timeout, ts_dir, work):
    work = os.path.join(work, engine)
    env = {k: v for k, v in os.environ.items() if k not in DROP_ENV}
    env.update(env_extra)
    launch = [exe] + (["--node"] if engine == "jsse" else [])
    argv = [a.replace("<WORK>", work) for a in argv]
    cmd = launch + argv
    if cols:
        cmd = [sys.executable, PTY, "--cols", str(cols), "--"] + cmd
    start = time.monotonic()
    try:
        r = subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, timeout=timeout)
        code, out, err = str(r.returncode), r.stdout, r.stderr
    except subprocess.TimeoutExpired as e:
        code, out, err = "timeout", e.stdout or b"", e.stderr or b""
    return {
        "exit": code,
        "stdout": normalise(out.decode("utf-8", "replace"), work, ts_dir),
        "stderr": normalise(err.decode("utf-8", "replace"), work, ts_dir),
        "secs": time.monotonic() - start,
    }


def run_case(case, engine, exe, ts_dir, work, timeout):
    name = case[0]
    if len(case) == 2:
        cwd = prepare(work, engine, name, "deps", ts_dir)
        argv = [os.path.join(cwd, "deps.mjs")]
        return execute(engine, exe, cwd, argv, case[1], 0, timeout, ts_dir, work)
    _, fixture, args, env_extra, cols = case
    cwd = prepare(work, engine, name, fixture, ts_dir)
    hereby = os.path.join(ts_dir, "node_modules", "hereby", "bin", "hereby.js")
    return execute(
        engine, exe, cwd, [hereby] + args, env_extra, cols, timeout, ts_dir, work
    )


def diff_parts(a, b):
    return {
        key: (a[key], b[key])
        for key in ("exit", "stdout", "stderr")
        if a[key] != b[key]
    }


def render(bad, limit):
    lines = []
    for key, (x, y) in bad.items():
        d = list(
            difflib.unified_diff(
                x.splitlines(), y.splitlines(), "node", "jsse", lineterm="", n=2
            )
        )
        lines.append(f"  {key} differs")
        lines += ["    " + row for row in d[:limit]]
        if len(d) > limit:
            lines.append(f"    ... {len(d) - limit} more diff lines")
    return "\n".join(lines)


def check_checkout(ts_dir):
    hereby = os.path.join(ts_dir, "node_modules", "hereby", "bin", "hereby.js")
    manifest = os.path.join(ts_dir, "package.json")
    if not os.path.isfile(hereby) or not os.path.isfile(manifest):
        return f"{ts_dir} has no node_modules/hereby (run npm ci in the TypeScript checkout)"
    with open(manifest, encoding="utf-8") as f:
        text = f.read()
    version = re.search(r'"version"\s*:\s*"([^"]+)"', text)
    version = version.group(1) if version else "unknown"
    if version != "6.0.3":
        return f"the hereby oracle targets TypeScript 6.0.3, found {version}"
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cases", nargs="*")
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--timeout", type=int, default=300)
    ap.add_argument("--ci", action="store_true")
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--require", action="store_true")
    ap.add_argument("--jsse")
    o = ap.parse_args()

    cases = HEREBY_CASES + DEPS_CASES
    if o.cases:
        unknown = [c for c in o.cases if c not in [c[0] for c in cases]]
        if unknown:
            sys.exit(f"unknown case: {', '.join(unknown)}")
        cases = [c for c in cases if c[0] in o.cases]
    if o.list:
        print("\n".join(c[0] for c in cases))
        return 0

    def skip(msg):
        print(("ERROR: " if o.require else "SKIP: ") + msg)
        return 3 if o.require else 0

    if not os.environ.get("TS_SRC_DIR"):
        return skip(
            "TS_SRC_DIR is not set; point it at a TypeScript v6.0.3 checkout with npm ci done to run the hereby oracle"
        )
    ts_dir = os.path.realpath(os.environ["TS_SRC_DIR"])
    problem = check_checkout(ts_dir)
    if problem:
        return skip(problem)
    node = shutil.which("node")
    if not node:
        return skip("node not found; the oracle needs it")
    given = o.jsse or os.environ.get("JSSE")
    jsse = os.path.abspath(given or os.path.join(ROOT, "target", "release", "jsse"))
    if not os.access(jsse, os.X_OK):
        if given:
            print(f"jsse binary not found or not executable: {jsse}", file=sys.stderr)
            return 1
        print("Building jsse (release)...", flush=True)
        subprocess.run(["cargo", "build", "--release"], cwd=ROOT, check=True)
    print(
        f"hereby from {ts_dir} (jsse: {jsse}, {len(cases)} cases, {o.jobs} jobs)",
        flush=True,
    )

    work = os.path.realpath(tempfile.mkdtemp(prefix="hereby-oracle-"))

    def one(case):
        ref = run_case(case, "node", node, ts_dir, work, o.timeout)
        got = run_case(case, "jsse", jsse, ts_dir, work, o.timeout)
        bad = diff_parts(ref, got)
        if not o.ci:
            print(f"{'ok  ' if not bad else 'FAIL'} {case[0]}", flush=True)
        return case, ref, got, bad

    try:
        with ThreadPoolExecutor(o.jobs) as pool:
            results = list(pool.map(one, cases))
        print(f"\n{'case':<26}{'node s':>9}{'jsse s':>9}  result")
        failed = 0
        for case, ref, got, bad in results:
            failed += bool(bad)
            what = "PASS" if not bad else "FAIL (" + ", ".join(bad) + ")"
            print(f"{case[0]:<26}{ref['secs']:>9.1f}{got['secs']:>9.1f}  {what}")
        for case, _, _, bad in results:
            if bad:
                print(f"\n{case[0]}:\n" + render(bad, 400 if o.ci else 40))
                if o.ci:
                    print(
                        f"::error title=hereby oracle::{case[0]} differs between node and jsse: {', '.join(bad)}"
                    )
        print(f"\nhereby oracle: {len(results) - failed}/{len(results)} passed")
        return 1 if failed else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
