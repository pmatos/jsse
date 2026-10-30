#!/usr/bin/env python3
"""Run TypeScript's `conformance` runner for parser, scanner, directives, pedantic and references on Node and jsse --node (issue #1280).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-conformance-1280-oracle.py [--chunks cf1,cf2,...] [--check-partition] [slice ...] [-- oracle options]

This is scripts/run-ts-conformance-oracle.py (loaded as a module: the compiler oracle's private working directory,
batches of many suites per mocha process, comparison of outcomes, error messages and tests/baselines/local, corrupted
reference control, known-failure ratchet) pointed at the slice rows of scripts/ts_conformance_slices_1280.py and the
ratchet file ts-suite/conformance-1280-known-failures.txt. Without slices it runs the five chunks cf1..cf5 (972 suites,
5832 tests, the directories of the issue) one oracle run after the other, so a crashed or interrupted chunk loses
nothing, and prints each wall time. Options after `--` go to the oracle (--jobs at most 4, --batch-size, --jsse, --record,
--verbose, --no-control, --list, ...). The exit status is 0 iff every run passes.

--check-partition (Node only) fails unless the chunks select disjoint suites that add up to `issue1280`.
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
import ts_conformance_slices_1280 as conf  # noqa: E402

KNOWN = os.path.join(ROOT, "ts-suite", "conformance-1280-known-failures.txt")

_spec = importlib.util.spec_from_file_location(
    "run_ts_conformance_oracle", os.path.join(SCRIPTS, "run-ts-conformance-oracle.py")
)
base = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(base)
base.conf = conf
base.KNOWN = KNOWN


def check_partition():
    """The chunks select disjoint suites that add up to `issue1280` (one dry run lists the runner)."""
    out = io.StringIO()
    base.configure()
    with contextlib.redirect_stdout(out):
        code = base.oracle(["--list", "prefix:0-9,A-Z"])
    if code != 0:
        sys.exit(f"cannot list the conformance suites:\n{out.getvalue()}")
    everything = [
        conf.Suite(conf.PREFIX + line, []) for line in out.getvalue().splitlines()[1:]
    ]
    whole = {s.key for s in conf.select("issue1280", everything)}
    seen, bad = {}, []
    for name in conf.CHUNKS:
        for suite in conf.select(name, everything):
            if suite.key in seen:
                bad.append(f"{suite.key!r} is in {seen[suite.key]} and {name}")
            seen[suite.key] = name
    bad += [f"not in any chunk: {k!r}" for k in sorted(whole - set(seen))]
    bad += [f"outside issue1280: {k!r}" for k in sorted(set(seen) - whole)]
    if not whole:
        bad.append("issue1280 lists no suites")
    print(f"chunks cover {len(seen)} of {len(whole)} suites of issue1280")
    print("\n".join(bad) or "partition OK")
    return 1 if bad else 0


def main(argv):
    cut = argv.index("--") if "--" in argv else len(argv)
    mine, rest = argv[:cut], argv[cut + 1 :]
    ap = argparse.ArgumentParser(description="conformance runner oracle, issue #1280")
    ap.add_argument("--chunks", default=",".join(conf.CHUNKS))
    ap.add_argument("--check-partition", action="store_true")
    ap.add_argument("slices", nargs="*")
    opts = ap.parse_args(mine)
    base.configure()
    if opts.check_partition:
        return check_partition()
    if opts.slices:
        return base.oracle(["--known", KNOWN, *rest, *opts.slices])
    failed = []
    for name in (c.strip() for c in opts.chunks.split(",") if c.strip()):
        if name not in conf.CHUNKS:
            sys.exit(f"unknown chunk {name!r}; chunks: {', '.join(conf.CHUNKS)}")
        t0 = time.time()
        code = base.oracle(["--known", KNOWN, *rest, name])
        print(
            f"=== {name}: exit {code} in {time.time() - t0:.0f}s wall time", flush=True
        )
        if code != 0:
            failed.append(name)
    print("chunks failed: " + (", ".join(failed) or "none"))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
