#!/usr/bin/env python3
"""Run TypeScript's `conformance` runner tests of statements, controlFlow and functions on Node and jsse --node (issue #1279).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-conformance-1279-oracle.py [--chunks st1,st2,st3,cf,fn] [--check-partition] [-- oracle options]

The comparison, baseline IO, corrupted-reference control and known-failure ratchet are those of
scripts/run-ts-compiler-oracle.py, pointed at the `conformance` runner exactly as
scripts/run-ts-conformance-expressions-oracle.py does (this script loads that module and reuses its oracle set-up); the
rows of the group are in scripts/ts_conformance_slices_1279.py and the ratchet file is
ts-suite/conformance-1279-known-failures.txt. The five chunks run one after the other, each one oracle run, and each
prints its wall time. Options after `--` go to the oracle (--jobs at most 4, --batch-size, --jsse, --record, --verbose,
--no-control, ...); --require is always passed. The exit status is 0 iff every chunk passes.

--check-partition (Node only) fails unless the chunks select disjoint suites that add up to `issue1279`.
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
import ts_conformance_slices_1275 as conf  # noqa: E402
import ts_conformance_slices_1279 as group  # noqa: E402

KNOWN = os.path.join(ROOT, "ts-suite", "conformance-1279-known-failures.txt")
MODULE = "ts_conformance_slices_1279"

_spec = importlib.util.spec_from_file_location(
    "run_ts_conformance_expressions_oracle",
    os.path.join(SCRIPTS, "run-ts-conformance-expressions-oracle.py"),
)
base = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(base)


def listing(oracle, spec):
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        code = base.run_oracle(oracle, ["--list", "--slices-module", MODULE, spec])
    if code:
        sys.exit(f"cannot list {spec}:\n{buf.getvalue()}")
    return buf.getvalue().splitlines()[1:]


def pinned_mismatches(whole, per_chunk):
    """Six tests per suite must add up to the passing counts ts-suite/oracle.json pins for the three directories."""
    table = {**conf.SLICES, **group.SLICES}
    counts = {
        "issue1279": len(whole),
        "statements": sum(len(per_chunk[c]) for c in ("st1", "st2", "st3")),
    }
    bad = []
    for name, suites in counts.items():
        pin = conf.pinned_expected(name, table)
        if pin is None or suites * 6 != pin:
            bad.append(f"{name}: {suites} suites x 6 tests differ from the pin {pin}")
    return bad


def check_partition(oracle):
    whole = listing(oracle, "issue1279")
    seen, bad = {}, []
    per_chunk = {chunk: listing(oracle, chunk) for chunk in group.CHUNKS}
    for chunk, suites in per_chunk.items():
        for suite in suites:
            if suite in seen:
                bad.append(f"{suite!r} is in {seen[suite]} and {chunk}")
            seen[suite] = chunk
    in_whole = set(whole)
    bad += [f"not in any chunk: {s!r}" for s in whole if s not in seen]
    bad += [f"outside issue1279: {s!r}" for s in seen if s not in in_whole]
    if not whole:
        bad.append("issue1279 lists no suites")
    bad += pinned_mismatches(whole, per_chunk)
    print(f"chunks cover {len(seen)} of {len(whole)} suites of issue1279")
    print("\n".join(bad) or "partition OK")
    return 1 if bad else 0


def main(argv):
    cut = argv.index("--") if "--" in argv else len(argv)
    mine, extra = argv[:cut], argv[cut + 1 :]
    ap = argparse.ArgumentParser(
        description="conformance statements/controlFlow/functions oracle"
    )
    ap.add_argument("--chunks", default=",".join(group.CHUNKS))
    ap.add_argument("--check-partition", action="store_true")
    opts = ap.parse_args(mine)
    chunks = [x.strip() for x in opts.chunks.split(",") if x.strip()]
    bad = [x for x in chunks if x not in group.CHUNKS]
    if bad:
        print(
            f"unknown chunks {bad}; choose from {','.join(group.CHUNKS)}",
            file=sys.stderr,
        )
        return 2
    oracle = base.load_oracle()
    oracle.KNOWN = KNOWN
    if opts.check_partition:
        return check_partition(oracle)
    failed = []
    for chunk in chunks:
        t0 = time.time()
        print(f"=== chunk {chunk}", flush=True)
        code = base.run_oracle(
            oracle, ["--slices-module", MODULE, "--known", KNOWN, *extra, chunk]
        )
        print(f"=== chunk {chunk}: exit {code} in {time.time() - t0:.0f}s", flush=True)
        if code:
            failed.append(chunk)
    print("chunks failed: " + (", ".join(failed) or "none"))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
