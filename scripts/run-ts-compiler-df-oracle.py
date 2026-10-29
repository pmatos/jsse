#!/usr/bin/env python3
"""Run the compiler-runner files D-F (issue #1270) through scripts/run-ts-compiler-oracle.py, chunk by chunk.

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-compiler-df-oracle.py [--check-partition] [chunk ...] [-- oracle options]

The chunks are the rows of scripts/ts_compiler_slices_df.py (d1..d6, e1..e6, f; default all, in that order). Each chunk is
one oracle run (Node and jsse --node, per-test outcome, error message and local-baseline comparison, corrupted-reference
control, known-failure ratchet) with the known failures of ts-suite/compiler-known-failures-d-f.txt, so a long run
reports per chunk and a failing chunk is repeated alone. Options after `--` go to the oracle (--jobs N at most 4,
--batch-size N, --jsse PATH, --record, --no-control, --verbose, --require). The exit status is 1 if any chunk failed.

--check-partition lists the suites of every chunk and of `d-f` and fails unless the chunks cover `d-f` exactly once
(needs only Node and TS_SRC_DIR).
"""

import os
import signal
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ts_compiler_slices_df as df  # noqa: E402

ORACLE = os.path.join(ROOT, "scripts", "run-ts-compiler-oracle.py")
KNOWN = os.path.join(ROOT, "ts-suite", "compiler-known-failures-d-f.txt")
BASE = [sys.executable, ORACLE, "--slices-module", "ts_compiler_slices_df"]


def run(args):
    """Run the oracle in its own process group, stream its output, kill the group on SIGINT/SIGTERM."""
    proc = subprocess.Popen(
        BASE + args, stdout=subprocess.PIPE, text=True, start_new_session=True
    )

    def stop(signum, _frame):
        try:
            os.killpg(proc.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        raise SystemExit(128 + signum)

    for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, stop)
    lines = []
    for line in proc.stdout:
        lines.append(line)
        print(line, end="", flush=True)
    proc.wait()
    return proc.returncode, "".join(lines)


def listing(spec):
    code, out = run(["--list", spec])
    if code != 0:
        sys.exit(f"cannot list {spec}:\n{out}")
    return out.splitlines()[1:]


def check_partition():
    whole = listing("d-f")
    seen = {}
    bad = []
    for name in df.CHUNKS:
        for suite in listing(name):
            if suite in seen:
                bad.append(f"{suite!r} is in {seen[suite]} and {name}")
            seen[suite] = name
    bad += [f"not in any chunk: {s!r}" for s in whole if s not in seen]
    bad += [f"outside d-f: {s!r}" for s in seen if s not in set(whole)]
    print(f"chunks cover {len(seen)} of {len(whole)} suites of d-f")
    print("\n".join(bad) or "partition OK")
    return 1 if bad else 0


def main():
    argv = sys.argv[1:]
    cut = argv.index("--") if "--" in argv else len(argv)
    mine, extra = argv[:cut], argv[cut + 1 :]
    if "--check-partition" in mine:
        return check_partition()
    names = mine or list(df.CHUNKS)
    unknown = [n for n in names if n not in df.CHUNKS and n != "d-f"]
    if unknown:
        sys.exit(f"unknown chunk {unknown}; chunks: {', '.join(df.CHUNKS)}, d-f")
    failed = []
    for name in names:
        print(f"=== chunk {name}: {df.SLICES[name]}", flush=True)
        code, _ = run(["--known", KNOWN] + extra + [name])
        if code != 0:
            failed.append(name)
    print("chunks failed: " + (", ".join(failed) or "none"))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
