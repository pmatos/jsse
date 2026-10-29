#!/usr/bin/env python3
"""Run TypeScript's `compiler` runner tests whose file names start with G to L on Node and jsse --node (issue #1271).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-compiler-gl-oracle.py [--letters g,h,...] [oracle options]

Runs scripts/run-ts-compiler-oracle.py once per letter (the rows of scripts/ts_compiler_slices_gl.py), one after the
other, against the known failures in ts-suite/compiler-known-failures-g-l.txt; a letter per run keeps a crashed or
interrupted run from losing the others, and prints the wall time of each. Other options go to the oracle
(--jobs, --batch-size, --jsse, --record, --verbose, ...). Exit status is 0 iff every letter passes (or the run is
skipped for want of TS_SRC_DIR). A letter's output is printed when its run ends (it can take 30 minutes); to watch
one live run scripts/run-ts-compiler-oracle.py directly with `--slices-module ts_compiler_slices_gl`.
"""

import argparse
import os
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import oracle_procs  # noqa: E402
import ts_compiler_slices_gl as gl  # noqa: E402

ORACLE = os.path.join(ROOT, "scripts", "run-ts-compiler-oracle.py")
KNOWN = os.path.join(ROOT, "ts-suite", "compiler-known-failures-g-l.txt")
# largeControlFlowGraph.ts (letter L) compiles a 10000-statement flow graph: about 830 s on a LTO-off jsse (under 270 s with LTO), so the oracle's
# 600 s mocha timeout would fail it. The other letters keep the oracle's defaults so a hang surfaces in 10 minutes.
SLOW_TEST_DEFAULTS = {"l": ("--test-timeout", "2400000", "--timeout", "7200")}


def main(argv):
    ap = argparse.ArgumentParser(description="G to L compiler oracle")
    ap.add_argument("--letters", default=",".join(gl.LETTERS))
    opts, rest = ap.parse_known_args(argv)
    letters = [x.strip().lower() for x in opts.letters.split(",") if x.strip()]
    bad = [x for x in letters if x not in gl.LETTERS]
    if bad:
        print(
            f"unknown letters {bad}; choose from {','.join(gl.LETTERS)}",
            file=sys.stderr,
        )
        return 2
    oracle_procs.install_cleanup()
    status = 0
    for letter in letters:
        cmd = [
            sys.executable,
            ORACLE,
            "--slices-module",
            "ts_compiler_slices_gl",
            "--known",
            KNOWN,
            *SLOW_TEST_DEFAULTS.get(letter, ()),
            *rest,
            letter,
        ]
        t0 = time.time()
        code, out, err = oracle_procs.run_group(cmd, ROOT, os.environ.copy(), 6 * 3600)
        print(
            f"=== letter {letter.upper()}: exit {code} in {time.time() - t0:.0f}s",
            flush=True,
        )
        sys.stdout.write(out.decode("utf-8", "replace"))
        sys.stdout.write(err.decode("utf-8", "replace"))
        sys.stdout.flush()
        if code != "0":
            status = 1
    return status


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
