#!/usr/bin/env python3
"""Run TypeScript's `conformance` runner on Node and on jsse --node and diff it per test (issue #1276).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-conformance-oracle.py [--dirs d1,d2] [--check-partition] [slice ...] [-- oracle options]

The `conformance` runner (tests/cases/conformance/**) is the compiler runner's `CompilerBaselineRunner` over another
directory, so this is scripts/run-ts-compiler-oracle.py (private working directory of symlinks, own
tests/baselines/local, batches of many suites per mocha process, the comparison, control and ratchet of
scripts/ts_compiler_report.py) pointed at it: the runner is `conformance`, the slice rows are those of
scripts/ts_conformance_slices.py (`dir:classes,decorators`, the four rows of the #1276 group and `issue1276`, plus the
compiler terms `first:N`, `prefix:`, `shard:`, `files:`, `match:`), the ratchet file is
ts-suite/conformance-1276-known-failures.txt and ts-suite/oracle.json pins the passing count per top-level directory.

Without slices it runs the issue's directories one after the other (override, decorators, esDecorators, classes; --dirs
picks others), each as its own oracle run so that a crashed or interrupted one loses nothing, and prints the wall time
of each. Options after `--` go to the oracle (--jobs at most 4, --batch-size, --jsse, --record, --verbose, --list, ...);
--require is always passed. The exit status is 0 iff every run passes. --check-partition (Node only) fails unless the
four directories select disjoint suites that add up to `issue1276`.
"""

import argparse
import importlib.util
import json
import os
import sys
import time

SCRIPTS = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(SCRIPTS)
sys.path.insert(0, SCRIPTS)
import ts_conformance_slices as conf  # noqa: E402

KNOWN = os.path.join(ROOT, "ts-suite", "conformance-1276-known-failures.txt")
SUITE_GREP = "^conformance tests conformance tests for "

_spec = importlib.util.spec_from_file_location(
    "run_ts_compiler_oracle", os.path.join(SCRIPTS, "run-ts-compiler-oracle.py")
)
compiler = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(compiler)


def pinned_expected(spec, table, runner="conformance"):
    """Passing tests ts-suite/oracle.json pins for a spec of `dir:` terms naming top-level directories, else None."""
    dirs = conf.top_dirs(spec, table)
    if dirs is None:
        return None
    try:
        with open(compiler.SUITES, encoding="utf-8") as f:
            buckets = json.load(f)["runners"][runner]["buckets"]
        return sum(buckets[d]["passed"] for d in dict.fromkeys(dirs))
    except (OSError, ValueError, KeyError):
        return None


def configure():
    compiler.slices = conf
    compiler.SUITE_GREP = SUITE_GREP
    compiler.CONFIG = {"runners": ["conformance"], "light": False}
    compiler.KNOWN = KNOWN
    compiler.pinned_expected = pinned_expected


def oracle(args):
    """One run of the compiler oracle's main() over the conformance runner; returns its exit status."""
    saved = sys.argv
    sys.argv = ["run-ts-conformance-oracle.py", "--require", *args]
    try:
        return compiler.main()
    except SystemExit as e:
        if isinstance(e.code, str):
            print(e.code, file=sys.stderr)
            return 1
        if isinstance(e.code, int) and e.code >= 128:
            raise
        return e.code or 0
    finally:
        sys.argv = saved


def check_partition():
    """The four directories select disjoint suites that add up to `issue1276` (one dry run lists the runner)."""
    import contextlib
    import io

    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        code = oracle(["--list", "prefix:0-9,A-Z"])
    if code != 0:
        sys.exit(f"cannot list the conformance suites:\n{out.getvalue()}")
    everything = [
        conf.Suite(conf.PREFIX + line, []) for line in out.getvalue().splitlines()[1:]
    ]
    whole = {s.key for s in conf.select("issue1276", everything)}
    seen, bad = {}, []
    for d in conf.GROUP_1276:
        for suite in conf.select(d, everything):
            if suite.key in seen:
                bad.append(f"{suite.key!r} is in {seen[suite.key]} and {d}")
            seen[suite.key] = d
    bad += [f"not in any directory: {k!r}" for k in sorted(whole - set(seen))]
    bad += [f"outside issue1276: {k!r}" for k in sorted(set(seen) - whole)]
    if not whole:
        bad.append("issue1276 lists no suites")
    print(f"directories cover {len(seen)} of {len(whole)} suites of issue1276")
    print("\n".join(bad) or "partition OK")
    return 1 if bad else 0


def main(argv):
    cut = argv.index("--") if "--" in argv else len(argv)
    mine, rest = argv[:cut], argv[cut + 1 :]
    ap = argparse.ArgumentParser(description="conformance runner oracle")
    ap.add_argument("--dirs", default=",".join(conf.GROUP_1276))
    ap.add_argument("--check-partition", action="store_true")
    ap.add_argument("slices", nargs="*")
    opts = ap.parse_args(mine)
    configure()
    if opts.check_partition:
        return check_partition()
    if opts.slices:
        return oracle(["--known", KNOWN, *rest, *opts.slices])
    dirs = [d.strip() for d in opts.dirs.split(",") if d.strip()]
    failed = []
    for d in dirs:
        t0 = time.time()
        code = oracle(["--known", KNOWN, *rest, f"dir:{d}"])
        print(
            f"=== {d}: exit {code} in {time.time() - t0:.0f}s wall time",
            flush=True,
        )
        if code != 0:
            failed.append(d)
    print("directories failed: " + (", ".join(failed) or "none"))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
