#!/usr/bin/env python3
"""Run TypeScript's `conformance` runner tests of tests/cases/conformance/expressions on Node and jsse --node (issue #1275).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-conformance-expressions-oracle.py [--chunks ex1,ex2,...] [--check-partition] [-- oracle options]

The comparison, baseline IO, corrupted-reference control and known-failure ratchet are those of
scripts/run-ts-compiler-oracle.py (see docs/agents/library-tests-typescript-compiler.md); this script loads that module,
points it at the `conformance` runner (scripts/ts_conformance_slices_1275.py: suite titles, `dir:` slices, the
`expressions` count pinned in ts-suite/oracle.json) and runs the group as four chunks (ex1..ex4, 429 suites, 2574 tests)
one after the other against ts-suite/conformance-expressions-known-failures.txt, so a crashed or interrupted chunk does
not lose the others. A chunk's output streams as it runs. Options after `--` go to the oracle (--jobs at most 4,
--batch-size, --jsse, --record, --verbose, --no-control, ...); --require is always passed, so a missing TS_SRC_DIR fails
instead of skipping. The exit status is 0 iff every chunk passes.

--check-partition lists the suites of every chunk and of `expressions` and fails unless the chunks cover it exactly once
(needs only Node and TS_SRC_DIR).
"""

import argparse
import importlib.util
import io
import os
import sys
import time
from contextlib import redirect_stdout

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCRIPTS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, SCRIPTS)
import ts_conformance_slices_1275 as conf  # noqa: E402

KNOWN = os.path.join(ROOT, "ts-suite", "conformance-expressions-known-failures.txt")


def load_oracle():
    spec = importlib.util.spec_from_file_location(
        "run_ts_compiler_oracle", os.path.join(SCRIPTS, "run-ts-compiler-oracle.py")
    )
    oracle = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(oracle)
    oracle.slices = conf
    oracle.CONFIG = conf.CONFIG
    oracle.SUITE_GREP = conf.SUITE_GREP
    oracle.KNOWN = KNOWN
    oracle.pinned_expected = lambda spec, table, runner=None: conf.pinned_expected(
        spec, table
    )
    return oracle


def run_oracle(oracle, args):
    argv = sys.argv
    sys.argv = ["run-ts-compiler-oracle.py", "--require", *args]
    try:
        return oracle.main()
    except SystemExit as e:
        return e.code if isinstance(e.code, int) else 1
    finally:
        sys.argv = argv


def listing(oracle, spec):
    buf = io.StringIO()
    with redirect_stdout(buf):
        code = run_oracle(
            oracle, ["--list", "--slices-module", "ts_conformance_slices_1275", spec]
        )
    if code:
        sys.exit(f"cannot list {spec}:\n{buf.getvalue()}")
    return buf.getvalue().splitlines()[1:]


def check_partition(oracle):
    whole = listing(oracle, "expressions")
    seen, bad = {}, []
    for chunk in conf.CHUNKS:
        for suite in listing(oracle, chunk):
            if suite in seen:
                bad.append(f"{suite!r} is in {seen[suite]} and {chunk}")
            seen[suite] = chunk
    in_whole = set(whole)
    bad += [f"not in any chunk: {s!r}" for s in whole if s not in seen]
    bad += [f"outside expressions: {s!r}" for s in seen if s not in in_whole]
    if not whole:
        bad.append("expressions lists no suites")
    print(f"chunks cover {len(seen)} of {len(whole)} suites of expressions")
    print("\n".join(bad) or "partition OK")
    return 1 if bad else 0


def main(argv):
    cut = argv.index("--") if "--" in argv else len(argv)
    mine, extra = argv[:cut], argv[cut + 1 :]
    ap = argparse.ArgumentParser(description="conformance/expressions oracle")
    ap.add_argument("--chunks", default=",".join(conf.CHUNKS))
    ap.add_argument("--check-partition", action="store_true")
    opts = ap.parse_args(mine)
    chunks = [x.strip() for x in opts.chunks.split(",") if x.strip()]
    bad = [x for x in chunks if x not in conf.CHUNKS]
    if bad:
        print(
            f"unknown chunks {bad}; choose from {','.join(conf.CHUNKS)}",
            file=sys.stderr,
        )
        return 2
    oracle = load_oracle()
    if opts.check_partition:
        return check_partition(oracle)
    failed = []
    for chunk in chunks:
        t0 = time.time()
        print(f"=== chunk {chunk}", flush=True)
        code = run_oracle(
            oracle,
            [
                "--slices-module",
                "ts_conformance_slices_1275",
                "--known",
                KNOWN,
                *extra,
                chunk,
            ],
        )
        print(f"=== chunk {chunk}: exit {code} in {time.time() - t0:.0f}s", flush=True)
        if code:
            failed.append(chunk)
    print("chunks failed: " + (", ".join(failed) or "none"))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
