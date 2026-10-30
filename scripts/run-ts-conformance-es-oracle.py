#!/usr/bin/env python3
"""Run TypeScript's `conformance` runner tests of es5, es7, es2016-es2025 and esnext on Node and jsse --node (issue #1278).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-conformance-es-oracle.py [--chunks es-a,es-b,...] [--check-partition] [-- oracle options]

The conformance wiring (suite titles, `dir:` slices, the pins of ts-suite/oracle.json) is that of
scripts/run-ts-conformance-expressions-oracle.py (#1275), which this script loads; the oracle behind it is
scripts/run-ts-compiler-oracle.py. Only the group differs: scripts/ts_conformance_slices_1278.py cuts the 13
directories (226 suites, 1356 tests) into four chunks (es-a..es-d) run one after the other against
ts-suite/conformance-es-known-failures.txt. Options after `--` go to the oracle (--jobs at most 4, --batch-size,
--jsse, --record, --verbose, --no-control, ...); --require is always passed. The exit status is 0 iff every chunk passes.

--check-partition (Node only) fails unless the chunks cover `issue1278` exactly once.
"""

import argparse
import importlib.util
import io
import os
import sys
import time
from contextlib import redirect_stdout

SCRIPTS = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(SCRIPTS)
sys.path.insert(0, SCRIPTS)
import ts_conformance_slices_1278 as group  # noqa: E402

KNOWN = os.path.join(ROOT, "ts-suite", "conformance-es-known-failures.txt")
MODULE = "ts_conformance_slices_1278"

_spec = importlib.util.spec_from_file_location(
    "run_ts_conformance_expressions_oracle",
    os.path.join(SCRIPTS, "run-ts-conformance-expressions-oracle.py"),
)
base = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(base)


def listing(oracle, spec):
    buf = io.StringIO()
    with redirect_stdout(buf):
        code = base.run_oracle(oracle, ["--list", "--slices-module", MODULE, spec])
    if code:
        sys.exit(f"cannot list {spec}:\n{buf.getvalue()}")
    return buf.getvalue().splitlines()[1:]


def check_partition(oracle):
    whole = listing(oracle, "issue1278")
    seen, bad = {}, []
    for chunk in group.CHUNKS:
        for suite in listing(oracle, chunk):
            if suite in seen:
                bad.append(f"{suite!r} is in {seen[suite]} and {chunk}")
            seen[suite] = chunk
    bad += [f"not in any chunk: {s!r}" for s in whole if s not in seen]
    bad += [f"outside issue1278: {s!r}" for s in seen if s not in set(whole)]
    if not whole:
        bad.append("issue1278 lists no suites")
    print(f"chunks cover {len(seen)} of {len(whole)} suites of issue1278")
    print("\n".join(bad) or "partition OK")
    return 1 if bad else 0


def main(argv):
    cut = argv.index("--") if "--" in argv else len(argv)
    mine, extra = argv[:cut], argv[cut + 1 :]
    ap = argparse.ArgumentParser(
        description="conformance es5/es7/es2016-es2025/esnext oracle"
    )
    ap.add_argument("--chunks", default=",".join(group.CHUNKS))
    ap.add_argument("--check-partition", action="store_true")
    opts = ap.parse_args(mine)
    chunks = [x.strip() for x in opts.chunks.split(",") if x.strip()]
    unknown = [x for x in chunks if x not in group.CHUNKS]
    if unknown:
        print(
            f"unknown chunks {unknown}; choose from {','.join(group.CHUNKS)}",
            file=sys.stderr,
        )
        return 2
    base.KNOWN = KNOWN
    oracle = base.load_oracle()
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
