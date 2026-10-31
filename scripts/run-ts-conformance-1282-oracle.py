#!/usr/bin/env python3
"""Run TypeScript's `conformance` runner for jsx, jsdoc, salsa and typings on Node and jsse --node (issue #1282).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-conformance-1282-oracle.py [--dirs d1,d2] [--check-partition] [slice ...] [-- oracle options]

This is scripts/run-ts-conformance-oracle.py (#1276; itself scripts/run-ts-compiler-oracle.py pointed at the
`conformance` runner: private working directory, batches of many suites per mocha process, per-test and
`tests/baselines/local` comparison, corrupted-reference control, known-failure ratchet) with the group of this issue:
the rows of scripts/ts_conformance_slices_1282.py (`jsx`, `jsdoc`, `salsa`, `typings`, `issue1282`) and the known
failures in ts-suite/conformance-1282-known-failures.txt. Without slices it runs the four directories one after the
other, each as its own oracle run so that a crashed or interrupted one loses nothing, and prints the wall time of each;
`--dirs` picks some. Options after `--` go to the oracle (--jobs at most 4, --batch-size, --jsse, --record, --verbose,
--list, ...); --test-timeout and --timeout default to 1800000 ms and 7200 s. The exit status is 0 iff every run passes. --check-partition (Node only) fails unless the four directories
select disjoint suites that add up to `issue1282`.
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
import ts_conformance_slices_1282 as conf  # noqa: E402

KNOWN = os.path.join(ROOT, "ts-suite", "conformance-1282-known-failures.txt")
# jsse needs about 1500 s of process time for the jsx directory on a loaded host: past the compiler oracle's 600 s test and
# 3600 s process defaults in a batch of 40 suites, so both limits are raised unless the caller passes its own.
DEFAULTS = (("--test-timeout", "1800000"), ("--timeout", "7200"))

_spec = importlib.util.spec_from_file_location(
    "run_ts_conformance_oracle", os.path.join(SCRIPTS, "run-ts-conformance-oracle.py")
)
wrapper = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(wrapper)
for _name in ("conf", "KNOWN", "configure", "oracle"):
    if not hasattr(wrapper, _name):
        sys.exit(f"run-ts-conformance-oracle.py has no {_name}: update this wrapper")
wrapper.conf = conf
wrapper.KNOWN = KNOWN


def check_partition():
    """The four directories select disjoint suites that add up to `issue1282` (one dry run lists the runner)."""
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        code = wrapper.oracle(["--list", "prefix:0-9,A-Z"])
    if code != 0:
        sys.exit(f"cannot list the conformance suites:\n{out.getvalue()}")
    everything = [
        conf.Suite(conf.PREFIX + line, []) for line in out.getvalue().splitlines()[1:]
    ]
    whole = {s.key for s in conf.select("issue1282", everything)}
    seen, bad = {}, []
    for d in conf.GROUP_1282:
        for suite in conf.select(d, everything):
            if suite.key in seen:
                bad.append(f"{suite.key!r} is in {seen[suite.key]} and {d}")
            seen[suite.key] = d
    bad += [f"not in any directory: {k!r}" for k in sorted(whole - set(seen))]
    bad += [f"outside issue1282: {k!r}" for k in sorted(set(seen) - whole)]
    if not whole:
        bad.append("issue1282 lists no suites")
    pinned = wrapper.pinned_expected("issue1282", conf.SLICES)
    if pinned is not None and len(whole) * conf.TESTS_PER_SUITE != pinned:
        bad.append(
            f"{len(whole)} suites of {conf.TESTS_PER_SUITE} tests differ from the {pinned} tests pinned in ts-suite/oracle.json"
        )
    print(f"directories cover {len(seen)} of {len(whole)} suites of issue1282")
    print("\n".join(bad) or "partition OK")
    return 1 if bad else 0


def main(argv):
    cut = argv.index("--") if "--" in argv else len(argv)
    mine, rest = argv[:cut], argv[cut + 1 :]
    ap = argparse.ArgumentParser(
        description="conformance jsx/jsdoc/salsa/typings oracle"
    )
    ap.add_argument("--dirs", default=",".join(conf.GROUP_1282))
    ap.add_argument("--check-partition", action="store_true")
    ap.add_argument("slices", nargs="*")
    opts = ap.parse_args(mine)
    for flag, value in DEFAULTS:
        if not any(a == flag or a.startswith(flag + "=") for a in rest):
            rest += [flag, value]
    wrapper.configure()
    if opts.check_partition:
        return check_partition()
    if opts.slices and opts.dirs != ap.get_default("dirs"):
        print("--dirs and slices are exclusive", file=sys.stderr)
        return 2
    if opts.slices:
        return wrapper.oracle(["--known", KNOWN, *rest, *opts.slices])
    dirs = list(dict.fromkeys(d.strip() for d in opts.dirs.split(",") if d.strip()))
    if not dirs:
        print("--dirs names no directory", file=sys.stderr)
        return 2
    unknown = [d for d in dirs if d not in conf.GROUP_1282]
    if unknown:
        print(
            f"unknown directories {unknown}; choose from {','.join(conf.GROUP_1282)}",
            file=sys.stderr,
        )
        return 2
    failed = []
    for d in dirs:
        t0 = time.time()
        print(f"=== directory {d}", flush=True)
        code = wrapper.oracle(["--known", KNOWN, *rest, d])
        print(f"=== {d}: exit {code} in {time.time() - t0:.0f}s wall time", flush=True)
        if code != 0:
            failed.append(d)
    print("directories failed: " + (", ".join(failed) or "none"))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
