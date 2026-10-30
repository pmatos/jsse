#!/usr/bin/env python3
"""Run TypeScript's `compiler` runner tests whose file names start with S to Z on Node and jsse --node (issue #1273).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-compiler-sz-oracle.py [--letters s,t,...] [--check-partition] [-- oracle options]

Runs scripts/run-ts-compiler-oracle.py once per letter (the rows of scripts/ts_compiler_slices_sz.py), one after the
other, against the known failures in ts-suite/compiler-known-failures-s-z.txt; a letter per run keeps a crashed or
interrupted run from losing the others. A letter's output is printed when its run ends, with its wall time; to watch
one live run scripts/run-ts-compiler-oracle.py directly with `--slices-module ts_compiler_slices_sz`. Options after
`--` go to the oracle (--jobs at most 4, --batch-size, --jsse, --record, --verbose, ...); --require is always passed,
so a missing TS_SRC_DIR fails instead of skipping. The exit status is 0 iff every letter passes.

--check-partition lists the suites of every letter and of `s-z` and fails unless the letters cover `s-z` exactly once
(needs only Node and TS_SRC_DIR).
"""

import argparse
import os
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import oracle_procs  # noqa: E402
import ts_compiler_slices_sz as sz  # noqa: E402

ORACLE = os.path.join(ROOT, "scripts", "run-ts-compiler-oracle.py")
KNOWN = os.path.join(ROOT, "ts-suite", "compiler-known-failures-s-z.txt")
BASE = [sys.executable, ORACLE, "--require", "--slices-module", "ts_compiler_slices_sz"]
RUN_TIMEOUT = 6 * 3600


def run(args):
    code, out, err = oracle_procs.run_group(
        BASE + args, ROOT, os.environ.copy(), RUN_TIMEOUT
    )
    return code, out.decode("utf-8", "replace"), err.decode("utf-8", "replace")


def listing(spec):
    code, out, err = run(["--list", spec])
    if code != "0":
        sys.exit(f"cannot list {spec}:\n{out}{err}")
    return out.splitlines()[1:]


def check_partition():
    whole = listing("s-z")
    seen = {}
    bad = []
    for letter in sz.LETTERS:
        for suite in listing(letter):
            if suite in seen:
                bad.append(f"{suite!r} is in {seen[suite]} and {letter}")
            seen[suite] = letter
    in_whole = set(whole)
    bad += [f"not in any letter: {s!r}" for s in whole if s not in seen]
    bad += [f"outside s-z: {s!r}" for s in seen if s not in in_whole]
    if not whole:
        bad.append("s-z lists no suites")
    print(f"letters cover {len(seen)} of {len(whole)} suites of s-z")
    print("\n".join(bad) or "partition OK")
    return 1 if bad else 0


def main(argv):
    cut = argv.index("--") if "--" in argv else len(argv)
    mine, extra = argv[:cut], argv[cut + 1 :]
    ap = argparse.ArgumentParser(description="S to Z compiler oracle")
    ap.add_argument("--letters", default=",".join(sz.LETTERS))
    ap.add_argument("--check-partition", action="store_true")
    opts = ap.parse_args(mine)
    letters = [x.strip().lower() for x in opts.letters.split(",") if x.strip()]
    bad = [x for x in letters if x not in sz.LETTERS]
    if bad:
        print(
            f"unknown letters {bad}; choose from {','.join(sz.LETTERS)}",
            file=sys.stderr,
        )
        return 2
    oracle_procs.install_cleanup()
    if opts.check_partition:
        return check_partition()
    failed = []
    for letter in letters:
        t0 = time.time()
        code, out, err = run(["--known", KNOWN, *extra, letter])
        print(
            f"=== letter {letter.upper()}: exit {code} in {time.time() - t0:.0f}s",
            flush=True,
        )
        sys.stdout.write(out)
        sys.stdout.write(err)
        sys.stdout.flush()
        if code != "0":
            failed.append(letter)
    print("letters failed: " + (", ".join(failed) or "none"))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
