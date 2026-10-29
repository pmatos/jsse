#!/usr/bin/env python3
"""Run slices of TypeScript's `fourslash` runners on Node and on jsse --node and diff them per test (issue #1288).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-fourslash-oracle.py [options] [slice ...]

The language-service harness: each test of tests/cases/fourslash (runner `fourslash`; `--runner fourslash-server`
for tests/cases/fourslash/server, which drive tsserver) is parsed (markers, ranges, files), run against an
in-process language service through `verify.*`/`edit.*`/`goTo.*`, and compares its baselines (quickinfo, references,
definitions, inlay hints, tsserver logs) with tests/baselines/reference. TS_SRC_DIR is a built TypeScript v6.0.3
checkout (node_modules/mocha, built/local/run.js); without it the script prints SKIP and exits 0 unless --require.
It is the machinery of scripts/run-ts-compiler-oracle.py (private working directory of symlinks, own
tests/baselines/local, batches of many tests per mocha process, process groups killed by scripts/oracle_procs.py,
the same comparison and ratchet in scripts/ts_compiler_report.py) over the slice table of
scripts/ts_fourslash_slices.py.

A slice (`first:N`, `prefix:A-C`, `shard:I/N`, `files:a.ts`, `match:RE`, a named row of SLICES, or a '+' union;
default first200) selects tests in the runner's enumeration order (a Node mocha dry run). Batches (--batch-size,
default 100 tests) are one mocha process per engine, at most 4 processes at once: loading the 13 MB runner bundle
costs 80 s on jsse and the first Program (lib.d.ts) 25 to 75 s per process. Compared: the outcome of every test; the
error message of a test failing on both; every file under tests/baselines/local, byte for byte. Node must pass
everything (ts-suite/oracle.json pins the passing tests per name range); a test passing on Node and failing on
jsse must be listed in ts-suite/fourslash-known-failures.txt (`full title<TAB>issue`), a listed test that passes
fails the run (ratchet). With --control (default on) five tests run again against corrupted references and
edited test files (scripts/ts_fourslash_control.py); both engines must fail them identically.

Options: --runner {fourslash,fourslash-server}, --jobs N (default 4, at most 4), --batch-size N, --test-timeout MS
(mocha -t, default 600000), --timeout SECONDS (per process, default 3600), --list, --require, --no-jsse,
--no-control, --record (rewrite the known failures of the slice), --jsse PATH (else $JSSE, else
target/release/jsse), --known FILE, --slices-module NAME (extra SLICES rows from scripts/NAME.py),
--keep-work DIR, --verbose.
"""

import argparse
import importlib
import importlib.util
import json
import os
import shutil
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor

SCRIPTS = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(SCRIPTS)
sys.path.insert(0, SCRIPTS)
import oracle_procs  # noqa: E402
import ts_fourslash_control as control  # noqa: E402
import ts_fourslash_slices as fs  # noqa: E402
from ts_compiler_report import (  # noqa: E402
    classify,
    counts,
    load_known,
    local_problems,
    merge,
    recordable,
    write_known,
)

_spec = importlib.util.spec_from_file_location(
    "run_ts_compiler_oracle", os.path.join(SCRIPTS, "run-ts-compiler-oracle.py")
)
compiler = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(compiler)

KNOWN = os.path.join(ROOT, "ts-suite", "fourslash-known-failures.txt")
MAX_JOBS = 4
REF = compiler.REF


def prepare(work, tag, ts_dir, runner, ref_dir=None, cases_dir=None):
    cwd = compiler.prepare(
        work, tag, ts_dir, ref_dir, {"runners": [runner], "light": False}
    )
    if cases_dir:
        link = os.path.join(cwd, "tests", "cases")
        os.remove(link)
        os.symlink(cases_dir, link)
    return cwd


def checkout_problem(ts_dir, runner):
    try:
        with open(os.path.join(ts_dir, "package.json"), encoding="utf-8") as f:
            version = json.load(f).get("version")
    except (OSError, ValueError):
        version = None
    needed = (
        "node_modules/mocha/bin/_mocha",
        "built/local/run.js",
        fs.RUNNERS[runner][0],
    )
    if version != "6.0.3" or not all(
        os.path.exists(os.path.join(ts_dir, p)) for p in needed
    ):
        return f"{ts_dir} is not a built TypeScript 6.0.3 checkout (needs {', '.join(needed)})"
    return None


def parse_args():
    ap = argparse.ArgumentParser()
    ap.add_argument("slices", nargs="*")
    ap.add_argument("--runner", choices=sorted(fs.RUNNERS), default="fourslash")
    ap.add_argument("--jobs", type=int, default=MAX_JOBS)
    ap.add_argument("--batch-size", type=int, default=100)
    ap.add_argument("--test-timeout", type=int, default=600000)
    ap.add_argument("--timeout", type=int, default=3600)
    for flag in (
        "--list",
        "--require",
        "--no-jsse",
        "--no-control",
        "--record",
        "--verbose",
    ):
        ap.add_argument(flag, action="store_true")
    ap.add_argument("--jsse")
    ap.add_argument("--known", default=KNOWN)
    ap.add_argument("--slices-module")
    ap.add_argument("--keep-work")
    return ap.parse_args()


def main():
    o = parse_args()
    runner = o.runner
    table = dict(fs.SLICES)
    if o.slices_module:
        table.update(importlib.import_module(o.slices_module).SLICES)

    def skip(msg):
        print(("ERROR: " if o.require else "SKIP: ") + msg)
        return 3 if o.require else 0

    if not os.environ.get("TS_SRC_DIR"):
        return skip(
            "TS_SRC_DIR is not set; point it at a built TypeScript v6.0.3 checkout"
        )
    ts_dir = os.path.realpath(os.environ["TS_SRC_DIR"])
    bad = checkout_problem(ts_dir, runner)
    if bad:
        return skip(bad)
    node = shutil.which("node")
    if not node:
        return skip("node not found")
    jsse = os.path.abspath(
        o.jsse
        or os.environ.get("JSSE")
        or os.path.join(ROOT, "target", "release", "jsse")
    )
    if not o.no_jsse and not o.list and not os.access(jsse, os.X_OK):
        print(f"jsse binary not found or not executable: {jsse}", file=sys.stderr)
        return 1

    oracle_procs.install_cleanup()
    work = os.path.realpath(tempfile.mkdtemp(prefix="ts-fourslash-oracle-"))
    jobs = max(1, min(o.jobs, MAX_JOBS))
    spec = "+".join(o.slices) if o.slices else "first200"
    try:
        listing = compiler.mocha_run(
            "node",
            node,
            prepare(work, "list", ts_dir, runner),
            f"^{runner} tests ",
            60000,
            600,
            dry=True,
        )
        if listing["report"] is None:
            print(f"cannot list the {runner} tests:\n" + listing["tail"])
            return 1
        everything = fs.tests_from_titles(
            ((t["fullTitle"], t["title"]) for t in listing["report"].get("tests", [])),
            runner,
        )
        try:
            chosen = fs.select(spec, everything, table)
        except ValueError as e:
            sys.exit(str(e))
        total = sum(len(t.tests) for t in chosen)
        print(
            f"{runner} {spec}: {len(chosen)} of {len(everything)} test files ({total} tests)",
            flush=True,
        )
        if o.list:
            print("\n".join(t.file for t in chosen))
            return 0
        bs = fs.batches(chosen, o.batch_size)
        print(
            f"{len(bs)} batches of up to {o.batch_size} tests, {jobs} jobs, jsse: {jsse}",
            flush=True,
        )

        def one(task):
            engine, tag, batch, ref_dir, cases_dir = task
            cwd = prepare(work, f"{tag}-{engine}", ts_dir, runner, ref_dir, cases_dir)
            exe = node if engine == "node" else jsse
            r = compiler.mocha_run(
                engine,
                exe,
                cwd,
                fs.grep_for(batch, runner),
                o.test_timeout,
                o.timeout,
            )
            print(f"done {tag} {engine} {r['secs']:.0f}s", flush=True)
            return r

        engines = ["jsse", "node"] if not o.no_jsse else ["node"]
        tasks = [(e, f"b{i}", b, None, None) for e in engines for i, b in enumerate(bs)]
        ctl, done, ref_dir = [], [], None
        if not o.no_control and not o.no_jsse and chosen:
            cdir = os.path.join(work, "control")
            os.makedirs(cdir)
            ctl, ref_dir, cases_dir, done = control.make_control(
                ts_dir, chosen, [t for t in everything if t not in chosen], cdir
            )
            tasks += [(e, "ctl", ctl, ref_dir, cases_dir) for e in engines]
        with ThreadPoolExecutor(jobs) as pool:
            res = list(pool.map(one, tasks))
        by = {(t[0], t[1]): r for t, r in zip(tasks, res)}
        node_res = merge([by[("node", f"b{i}")] for i in range(len(bs))])
        jsse_res = (
            merge([by[("jsse", f"b{i}")] for i in range(len(bs))])
            if not o.no_jsse
            else None
        )

        known = load_known(o.known)
        in_slice = {x for t in chosen for x in t.tests}
        mine = {t: w for t, w in known.items() if t in in_slice}
        nc = counts(node_res)
        print(
            f"\nNode: {nc['passing']} passing, {nc['failing']} failing, {nc['pending']} pending in {node_res['secs']:.0f}s"
        )
        problems = [f"Node: {b}" for b in node_res["bad"]]
        if sum(nc.values()) != total:
            problems.append(f"Node ran {sum(nc.values())} tests, the slice has {total}")
        want = compiler.pinned_expected(spec, table, runner)
        if want is not None and nc["passing"] != want:
            problems.append(
                f"ts-suite/oracle.json pins {want} passing tests for {spec}, Node ran {nc['passing']}"
            )
        if jsse_res is not None:
            jc = counts(jsse_res)
            print(
                f"jsse: {jc['passing']} passing, {jc['failing']} failing, {jc['pending']} pending in {jsse_res['secs']:.0f}s of process time"
            )
            problems += [f"jsse: {b}" for b in jsse_res["bad"]]
            if sum(jc.values()) != total:
                problems.append(
                    f"jsse ran {sum(jc.values())} tests, the slice has {total}"
                )
            problems += classify(node_res, jsse_res, mine)
            problems += local_problems(node_res, jsse_res, os.path.join(ts_dir, REF))
            if o.verbose:
                print("\n".join(f"known failure: {t} ({w})" for t, w in mine.items()))
        if ctl and jsse_res is not None:
            cn, cj = merge([by[("node", "ctl")]]), merge([by[("jsse", "ctl")]])
            print(
                f"control ({'; '.join(done)}): Node fails {counts(cn)['failing']} of {sum(counts(cn).values())}, jsse fails {counts(cj)['failing']}"
            )
            if o.verbose:
                for t, m in sorted(cn["errors"].items()):
                    print(
                        f"control failure: {t}\n    " + m[:400].replace("\n", "\n    ")
                    )
            cp = [
                f"control: {p}"
                for p in classify(cn, cj, set())
                if not p.startswith("fails on Node itself")
            ]
            cp += [f"control: {p}" for p in local_problems(cn, cj, ref_dir)]
            cp += [f"control: {b}" for b in cn["bad"] + cj["bad"]]
            if counts(cn)["failing"] < len(ctl):
                cp.append(
                    "control: a control test passed on Node (the control checks nothing)"
                )
            problems += cp
        if o.record and jsse_res is not None:
            if recordable(problems):
                print("refusing to record:\n  " + "\n  ".join(recordable(problems)))
                return 1
            fails = {t for (t, k) in jsse_res["outcomes"] if k == "failing"} - {
                t for (t, k) in node_res["outcomes"] if k == "failing"
            }
            new = {t: w for t, w in known.items() if t not in in_slice}
            new.update(
                {
                    t: known.get(t, "TODO: file the bug and put its number here")
                    for t in fails
                }
            )
            write_known(o.known, new, "run-ts-fourslash-oracle.py")
            print(f"wrote {o.known}: {len(new)} known failures")
            return 0
        for p in problems:
            print("  " + p)
        print("fourslash oracle: " + ("FAIL" if problems else "PASS"))
        return 1 if problems else 0
    finally:
        if o.keep_work:
            shutil.copytree(work, o.keep_work, dirs_exist_ok=True, symlinks=True)
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
