#!/usr/bin/env python3
"""Compare the error stacks hereby prints for a Herebyfile that fails to load, Node against jsse --node (issue #1251).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-hereby-stacks-oracle.py [options] [case ...]

Same inputs, comparison and normalisation as scripts/run-hereby-oracle.py (loaded as a module; only
`node:internal` frames are dropped, every user frame and `at async` frame must match), over the
`stack-*` fixtures of scripts/hereby-fixtures: errors thrown from a nested module, through class
methods, after top-level await, through a CommonJS require, with a `cause`, from hereby's own
validation, a thrown non-Error, and a missing module or package (`Error [ERR_...]` headers).
ts-suite/hereby-stacks-known-failures.json lists the cases that still differ (case -> issue); a
listed case that now agrees, or an unlisted case that differs, exits 1.

Options: --jobs N (default 4), --timeout SECONDS, --list, --require, --jsse PATH (else $JSSE,
else target/release/jsse).
"""

import argparse
import importlib.util
import json
import os
import shutil
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
KNOWN = os.path.join(ROOT, "ts-suite", "hereby-stacks-known-failures.json")

CASES = [
    ("stack-import-throws", "stack-import-throws", [], {}, 0),
    ("stack-class", "stack-class", [], {}, 0),
    ("stack-tla", "stack-tla", [], {}, 0),
    ("stack-cjs", "stack-cjs", [], {}, 0),
    ("stack-cause", "stack-cause", [], {}, 0),
    ("stack-bad-task", "stack-bad-task", [], {}, 0),
    ("stack-not-error", "stack-not-error", [], {}, 0),
    ("stack-missing-import", "stack-missing-import", [], {}, 0),
    ("stack-missing-package", "stack-missing-package", [], {}, 0),
    ("stack-default-limit", "stack-import-throws", [], {}, 0),
    ("stack-pty", "stack-import-throws", [], {}, 100),
    ("stack-colour", "stack-import-throws", [], {"FORCE_COLOR": "1"}, 0),
    ("stack-native", "stack-native", [], {}, 0),
    ("stack-syntax", "stack-syntax", [], {}, 0),
    ("stack-missing-export", "stack-missing-export", [], {}, 0),
]


UNLAUNCHED = ("stack-default-limit",)
LAUNCHER = (
    "Error.stackTraceLimit = 200;\n"
    "process.argv.splice(1, 1);\n"
    "await import(process.argv[1]);\n"
)


def load_base():
    spec = importlib.util.spec_from_file_location(
        "run_hereby_oracle", os.path.join(ROOT, "scripts", "run-hereby-oracle.py")
    )
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def run_case(base, case, engine, exe, ts_dir, work, timeout):
    """base.run_case, but through a launcher that lifts Error.stackTraceLimit so that
    the different number of node:internal frames cannot cut user frames off one side."""
    if case[0] in UNLAUNCHED:
        return base.run_case(case, engine, exe, ts_dir, work, timeout)
    _, fixture, args, env_extra, cols = case
    cwd = base.prepare(work, engine, case[0], fixture, ts_dir)
    launcher = os.path.join(work, engine, case[0] + "-launcher.mjs")
    with open(launcher, "w", encoding="utf-8") as f:
        f.write(LAUNCHER)
    hereby = os.path.join(ts_dir, "node_modules", "hereby", "bin", "hereby.js")
    return base.execute(
        engine,
        exe,
        cwd,
        [launcher, hereby] + args,
        env_extra,
        cols,
        timeout,
        ts_dir,
        work,
    )


def read_known(path=KNOWN):
    with open(path, encoding="utf-8") as f:
        return json.load(f)["known_failures"]


def judge(name, differs, known):
    """Return an error message for a case whose outcome breaks the ratchet, else None."""
    if differs and name not in known:
        return "differs between node and jsse"
    if not differs and name in known:
        return f"is listed as a known failure ({known[name]}) but now agrees; remove it from the list"
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cases", nargs="*")
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--timeout", type=int, default=300)
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--require", action="store_true")
    ap.add_argument("--jsse")
    o = ap.parse_args()

    names = [c[0] for c in CASES]
    unknown = [c for c in o.cases if c not in names]
    if unknown:
        sys.exit(f"unknown case: {', '.join(unknown)}")
    cases = [c for c in CASES if not o.cases or c[0] in o.cases]
    if o.list:
        print("\n".join(c[0] for c in cases))
        return 0

    base = load_base()
    known = read_known()
    stale = [k for k in known if k not in names]
    if stale:
        print(f"ERROR: known failures name no case: {', '.join(stale)}")
        return 1

    def skip(msg):
        print(("ERROR: " if o.require else "SKIP: ") + msg)
        return 3 if o.require else 0

    if not os.environ.get("TS_SRC_DIR"):
        return skip(
            "TS_SRC_DIR is not set; point it at a TypeScript v6.0.3 checkout with npm ci done"
        )
    ts_dir = os.path.realpath(os.environ["TS_SRC_DIR"])
    problem = base.check_checkout(ts_dir)
    if problem:
        return skip(problem)
    node = shutil.which("node")
    if not node:
        return skip("node not found; the oracle needs it")
    given = o.jsse or os.environ.get("JSSE")
    jsse = os.path.abspath(given or os.path.join(ROOT, "target", "release", "jsse"))
    if not os.access(jsse, os.X_OK):
        print(f"jsse binary not found or not executable: {jsse}", file=sys.stderr)
        return 1
    print(f"hereby stacks from {ts_dir} (jsse: {jsse}, {len(cases)} cases)", flush=True)

    work = os.path.realpath(tempfile.mkdtemp(prefix="hereby-stacks-oracle-"))

    def one(case):
        ref = run_case(base, case, "node", node, ts_dir, work, o.timeout)
        got = run_case(base, case, "jsse", jsse, ts_dir, work, o.timeout)
        return case, base.diff_parts(ref, got)

    failed = 0
    try:
        with ThreadPoolExecutor(o.jobs) as pool:
            results = list(pool.map(one, cases))
        for case, bad in results:
            problem = judge(case[0], bool(bad), known)
            state = "known" if case[0] in known and bad else "ok"
            if problem:
                failed += 1
                state = "FAIL"
            print(f"{state:<6}{case[0]}")
            if problem:
                print(f"  {case[0]} {problem}")
            if bad and (problem or o.cases):
                print(base.render(bad, 40))
    finally:
        shutil.rmtree(work, ignore_errors=True)
    print(f"\nhereby stacks oracle: {len(results) - failed}/{len(results)} as expected")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
