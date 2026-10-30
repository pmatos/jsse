#!/usr/bin/env python3
"""Run TypeScript's `conformance` runner tests of tests/cases/conformance/es6 on Node and jsse --node (issue #1277).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-conformance-es6-oracle.py [--chunks es1,es2,...] [--check-partition] [-- oracle options]

The comparison, baseline IO, corrupted-reference control and known-failure ratchet are those of
scripts/run-ts-compiler-oracle.py, pointed at the `conformance` runner by scripts/run-ts-conformance-oracle.py (#1276),
whose `configure()` and `oracle()` this script loads. The `es6` directory (1315 suites, 7890 tests) is run as five chunks
(es1..es5, rows of scripts/ts_conformance_slices_1277.py) one after the other against
ts-suite/conformance-es6-known-failures.txt, so a crashed or interrupted chunk does not lose the others. Options after
`--` go to the oracle (--jobs at most 4, --batch-size, --jsse, --record, --verbose, --no-control, ...); --require is
always passed. The exit status is 0 iff every chunk passes. --check-partition (Node only) fails unless the chunks cover
`es6` exactly once.
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
import ts_conformance_slices_1277 as es6  # noqa: E402

KNOWN = os.path.join(ROOT, "ts-suite", "conformance-es6-known-failures.txt")
MODULE = ["--slices-module", "ts_conformance_slices_1277"]

_spec = importlib.util.spec_from_file_location(
    "run_ts_conformance_oracle", os.path.join(SCRIPTS, "run-ts-conformance-oracle.py")
)
base = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(base)


def listing(spec):
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        code = base.oracle(["--list", *MODULE, spec])
    if code:
        sys.exit(f"cannot list {spec}:\n{out.getvalue()}")
    return out.getvalue().splitlines()[1:]


def check_partition():
    whole = listing("es6")
    seen, bad = {}, []
    for chunk in es6.CHUNKS:
        for suite in listing(chunk):
            if suite in seen:
                bad.append(f"{suite!r} is in {seen[suite]} and {chunk}")
            seen[suite] = chunk
    in_whole = set(whole)
    bad += [f"not in any chunk: {s!r}" for s in whole if s not in seen]
    bad += [f"outside es6: {s!r}" for s in seen if s not in in_whole]
    if not whole:
        bad.append("es6 lists no suites")
    print(f"chunks cover {len(seen)} of {len(whole)} suites of es6")
    print("\n".join(bad) or "partition OK")
    return 1 if bad else 0


def main(argv):
    cut = argv.index("--") if "--" in argv else len(argv)
    mine, rest = argv[:cut], argv[cut + 1 :]
    ap = argparse.ArgumentParser(description="conformance/es6 oracle")
    ap.add_argument("--chunks", default=",".join(es6.CHUNKS))
    ap.add_argument("--check-partition", action="store_true")
    opts = ap.parse_args(mine)
    chunks = [c.strip() for c in opts.chunks.split(",") if c.strip()]
    unknown = [c for c in chunks if c not in es6.CHUNKS]
    if unknown:
        print(
            f"unknown chunks {unknown}; choose from {','.join(es6.CHUNKS)}",
            file=sys.stderr,
        )
        return 2
    base.configure()
    if opts.check_partition:
        return check_partition()
    failed = []
    for chunk in chunks:
        t0 = time.time()
        print(f"=== chunk {chunk}", flush=True)
        code = base.oracle([*MODULE, "--known", KNOWN, *rest, chunk])
        print(f"=== chunk {chunk}: exit {code} in {time.time() - t0:.0f}s", flush=True)
        if code:
            failed.append(chunk)
    print("chunks failed: " + (", ".join(failed) or "none"))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
