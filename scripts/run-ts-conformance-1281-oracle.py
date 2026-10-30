#!/usr/bin/env python3
"""Run TypeScript's `conformance` runner for declarationEmit, emitter, ambient and additionalChecks on Node and jsse --node (issue #1281).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-conformance-1281-oracle.py [--dirs d1,d2] [--check-partition] [slice ...] [-- oracle options]

This is scripts/run-ts-conformance-oracle.py (issue #1276, itself scripts/run-ts-compiler-oracle.py pointed at the
`conformance` runner) with the slice rows of scripts/ts_conformance_slices_1281.py (`declarationEmit`, `emitter`,
`ambient`, `additionalChecks`, `issue1281`) and the known failures in ts-suite/conformance-1281-known-failures.txt:
378 tests in 63 suites, the passing counts ts-suite/oracle.json pins for the four directories. Without slices it runs the
directories one after the other, each as its own oracle run, and prints the wall time of each. Options after `--` go to
the oracle (--jobs at most 4, --batch-size, --jsse, --record, --verbose, --list, ...); --require is always passed. The
exit status is 0 iff every run passes. --check-partition (Node only) fails unless the four directories select disjoint
suites that add up to `issue1281`.
"""

import argparse
import contextlib
import importlib.util
import io
import os
import sys
import time

SCRIPTS = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(SCRIPTS)
sys.path.insert(0, SCRIPTS)
import ts_conformance_slices_1281 as conf  # noqa: E402

KNOWN = os.path.join(ROOT, "ts-suite", "conformance-1281-known-failures.txt")

_spec = importlib.util.spec_from_file_location(
    "run_ts_conformance_oracle", os.path.join(SCRIPTS, "run-ts-conformance-oracle.py")
)
base = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(base)
base.conf = conf
base.KNOWN = KNOWN


def check_partition():
    """The four directories select disjoint suites that add up to `issue1281` (one dry run lists the runner)."""
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        base.configure()
        code = base.oracle(["--list", "prefix:0-9,A-Z"])
    if code != 0:
        sys.exit(f"cannot list the conformance suites:\n{out.getvalue()}")
    everything = [
        conf.Suite(conf.PREFIX + line, []) for line in out.getvalue().splitlines()[1:]
    ]
    whole = {s.key for s in conf.select("issue1281", everything)}
    seen, bad = {}, []
    for d in conf.GROUP_1281:
        for suite in conf.select(d, everything):
            if suite.key in seen:
                bad.append(f"{suite.key!r} is in {seen[suite.key]} and {d}")
            seen[suite.key] = d
    bad += [f"not in any directory: {k!r}" for k in sorted(whole - set(seen))]
    bad += [f"outside issue1281: {k!r}" for k in sorted(set(seen) - whole)]
    if not whole:
        bad.append("issue1281 lists no suites")
    print(f"directories cover {len(seen)} of {len(whole)} suites of issue1281")
    print("\n".join(bad) or "partition OK")
    return 1 if bad else 0


def main(argv):
    cut = argv.index("--") if "--" in argv else len(argv)
    mine, rest = argv[:cut], argv[cut + 1 :]
    ap = argparse.ArgumentParser(description="conformance runner oracle, issue 1281")
    ap.add_argument("--dirs", default=",".join(conf.GROUP_1281))
    ap.add_argument("--check-partition", action="store_true")
    ap.add_argument("slices", nargs="*")
    opts = ap.parse_args(mine)
    if opts.check_partition:
        return check_partition()
    base.configure()
    if opts.slices:
        return base.oracle(["--known", KNOWN, *rest, *opts.slices])
    failed = []
    for d in [d.strip() for d in opts.dirs.split(",") if d.strip()]:
        t0 = time.time()
        code = base.oracle(["--known", KNOWN, *rest, f"dir:{d}"])
        print(f"=== {d}: exit {code} in {time.time() - t0:.0f}s wall time", flush=True)
        if code != 0:
            failed.append(d)
    print("directories failed: " + (", ".join(failed) or "none"))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
