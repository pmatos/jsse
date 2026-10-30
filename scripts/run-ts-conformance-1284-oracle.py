#!/usr/bin/env python3
"""Run TypeScript's `conformance` runner tests of async, asyncGenerators, generators, enums, constEnums, interfaces,
Symbols and the loose root files on Node and jsse --node (issue #1284).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-conformance-1284-oracle.py [--chunks as1,as2,if,mix] [--check-partition] [-- oracle options]

The comparison, baseline IO, corrupted-reference control and known-failure ratchet are those of
scripts/run-ts-compiler-oracle.py, pointed at the `conformance` runner exactly as
scripts/run-ts-conformance-expressions-oracle.py does (this script loads that module and reuses its oracle set-up); the
rows of the group are in scripts/ts_conformance_slices_1284.py and the ratchet file is
ts-suite/conformance-1284-known-failures.txt. The four chunks run one after the other, each one oracle run, and each
prints its wall time. Options after `--` go to the oracle (--jobs at most 4, --batch-size, --jsse, --record, --verbose,
--no-control, ...); --require is always passed. The exit status is 0 iff every chunk passes.

--check-partition (Node only) fails unless (1) the chunks select disjoint suites that add up to `issue1284`, (2) those
suites are the 6-test suites ts-suite/oracle.json pins for the group, (3) the root files are the files at the root of
tests/cases/conformance and nothing else of that name, and (4) every top-level entry of tests/cases/conformance and every
bucket of oracle.json belongs to exactly one of the conformance issues #1274 to #1284 (OWNERS).
"""

import argparse
import contextlib
import importlib.util
import io
import json
import os
import sys
import time

SCRIPTS = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(SCRIPTS)
sys.path.insert(0, SCRIPTS)
import ts_conformance_slices_1275 as conf  # noqa: E402
import ts_conformance_control_1284 as control  # noqa: E402
import ts_conformance_slices_1284 as group  # noqa: E402

KNOWN = os.path.join(ROOT, "ts-suite", "conformance-1284-known-failures.txt")
MODULE = "ts_conformance_slices_1284"
TESTS_PER_SUITE = 6
LISTED = "conformance tests for "

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


def pinned_tests(oracle_json=conf.SUITES):
    """(tests pinned for the group, buckets of oracle.json, passing tests of no bucket = the root files)."""
    with open(oracle_json, encoding="utf-8") as f:
        runner = json.load(f)["runners"][conf.RUNNER]
    buckets = {k: v["passed"] for k, v in runner["buckets"].items()}
    loose = runner["passed"] - sum(buckets.values())
    return sum(buckets[d] for d in group.GROUP_DIRS) + loose, buckets, loose


def owner_problems(entries, buckets, loose):
    """Problems of the OWNERS table against the top-level `entries` of the conformance directory and the pins."""
    claimed, bad = {}, []
    for issue, names in group.OWNERS.items():
        for name in names:
            if name in claimed:
                bad.append(f"{name} is run by #{claimed[name]} and #{issue}")
            claimed[name] = issue
    bad += [
        f"on disk, run by no issue: {e}" for e in sorted(set(entries) - set(claimed))
    ]
    bad += [f"owned but not on disk: {n}" for n in sorted(set(claimed) - set(entries))]
    dirs = {n for n in claimed if not n.endswith(".ts")}
    bad += [
        f"bucket of oracle.json run by no issue: {b}"
        for b in sorted(set(buckets) - dirs)
    ]
    bad += [
        f"owned directory without a pinned bucket: {d}"
        for d in sorted(dirs - set(buckets))
    ]
    files = [n for n in claimed if n.endswith(".ts")]
    if loose != len(files) * TESTS_PER_SUITE:
        bad.append(
            f"{len(files)} root files x {TESTS_PER_SUITE} tests differ from the {loose} unbucketed pinned tests"
        )
    return bad


def check_partition(oracle):
    whole = listing(oracle, "issue1284")
    seen, bad = {}, []
    per_chunk = {chunk: listing(oracle, chunk) for chunk in group.CHUNKS}
    for chunk, suites in per_chunk.items():
        for suite in suites:
            if suite in seen:
                bad.append(f"{suite!r} is in {seen[suite]} and {chunk}")
            seen[suite] = chunk
    in_whole = set(whole)
    bad += [f"not in any chunk: {s!r}" for s in whole if s not in seen]
    bad += [f"outside issue1284: {s!r}" for s in seen if s not in in_whole]
    if not whole:
        bad.append("issue1284 lists no suites")
    pin, buckets, loose = pinned_tests()
    if len(whole) * TESTS_PER_SUITE != pin:
        bad.append(
            f"issue1284: {len(whole)} suites x {TESTS_PER_SUITE} tests differ from the pin {pin}"
        )
    files = {s.removeprefix(LISTED).split(" (")[0] for s in whole}
    for f in group.ROOT_FILES:
        if conf.DIR + f not in files:
            bad.append(f"root file {f} has no suite at {conf.DIR}{f}")
    bad += [
        f"{p} has the name of a root file but is not at the root"
        for p in sorted(files)
        if p.rsplit("/", 1)[-1] in group.ROOT_FILES
        and p.count("/") != conf.DIR.count("/")
    ]
    entries = os.listdir(os.path.join(os.environ["TS_SRC_DIR"], conf.DIR))
    bad += owner_problems(entries, buckets, loose)
    print(f"chunks cover {len(seen)} of {len(whole)} suites of issue1284")
    print("\n".join(bad) or "partition OK")
    return 1 if bad else 0


def main(argv):
    cut = argv.index("--") if "--" in argv else len(argv)
    mine, extra = argv[:cut], argv[cut + 1 :]
    ap = argparse.ArgumentParser(
        description="conformance async/enums/interfaces/Symbols oracle"
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
    oracle.ts_compiler_control = control
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
