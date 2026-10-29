#!/usr/bin/env python3
"""Run slices of TypeScript's `project` runner on Node and on jsse --node and diff them per test (issue #1286).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-project-oracle.py [options] [slice ...]

The `project` runner (tests/cases/project/*.json, 316 files, 2528 tests) compiles a small project from
tests/cases/projects with the options of the json file (out/outFile/outDir, sourceMap, mapRoot, sourceRoot,
declaration; resolveMapRoot/resolveSourceRoot) once as commonjs and once as amd on an in-memory file system, and
compares the module resolution trace, the errors, every emitted file (js, js.map, d.ts) and the declaration
compile errors with tests/baselines/reference/project/<name>/<node|amd>/. TS_SRC_DIR is a built TypeScript v6.0.3
checkout (node_modules/mocha, built/local/run.js); without it the script prints SKIP and exits 0 unless --require.
It is the machinery of scripts/run-ts-compiler-oracle.py (private working directory of symlinks, own
tests/baselines/local, batches per mocha process, process groups killed by scripts/oracle_procs.py, the comparison
and ratchet of scripts/ts_compiler_report.py) over the suites of scripts/ts_project_slices.py.

A slice (`first:N`, `prefix:A-C`, `shard:I/N`, `files:a.json`, `match:RE`, a named row of SLICES, or a '+' union;
default `matrix`, the #1286 selection: out*, sourcemap*, mapRoot*/maprootUrl*, sourceRoot*/sourcerootUrl*, 228 files,
1824 tests) selects suites (a json file in one module kind, four tests). Batches (--batch-size suites, default 60)
are one mocha process per engine, at most 4 processes at once. Compared: the outcome of every test, the error
message of a test failing on both, every file under tests/baselines/local byte for byte. Node must pass everything
(ts-suite/oracle.json pins 2528 passing project tests; a bare `all` run checks it); a test passing on Node and
failing on jsse must be listed in ts-suite/project-1286-known-failures.txt (`full title<TAB>issue`), a listed test
that passes fails the run (ratchet). With --control (default on) five suites run again against corrupted
references (scripts/ts_project_control.py); both engines must fail them identically.

Options: --jobs N (default 4, at most 4), --batch-size N, --test-timeout MS (mocha -t, default 600000),
--timeout SECONDS (per process, default 3600), --list, --require, --no-jsse, --no-control, --record (rewrite the
known failures of the slice), --jsse PATH (else $JSSE, else target/release/jsse), --known FILE,
--slices-module NAME (extra SLICES rows from scripts/NAME.py), --keep-work DIR, --verbose.
"""

import argparse
import importlib
import importlib.util
import json
import os
import re
import shutil
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor

SCRIPTS = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(SCRIPTS)
sys.path.insert(0, SCRIPTS)
import oracle_procs  # noqa: E402
import ts_project_control as control  # noqa: E402
import ts_project_slices as ps  # noqa: E402
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

KNOWN = os.path.join(ROOT, "ts-suite", "project-1286-known-failures.txt")
MAX_JOBS = 4
REF = compiler.REF
CONFIG = {"runners": ["project"], "light": False}


def pinned_expected(chosen, everything):
    """Passing tests ts-suite/oracle.json pins when the slice is the whole runner, else None."""
    if len(chosen) != len(everything):
        return None
    try:
        with open(compiler.SUITES, encoding="utf-8") as f:
            return json.load(f)["runners"]["project"]["passed"]
    except (OSError, ValueError, KeyError):
        return None


def checkout_problem(ts_dir):
    try:
        with open(os.path.join(ts_dir, "package.json"), encoding="utf-8") as f:
            version = json.load(f).get("version")
    except (OSError, ValueError):
        version = None
    needed = (
        "node_modules/mocha/bin/_mocha",
        "built/local/run.js",
        "tests/cases/project",
        "tests/cases/projects",
    )
    if version != "6.0.3" or not all(
        os.path.exists(os.path.join(ts_dir, p)) for p in needed
    ):
        return f"{ts_dir} is not a built TypeScript 6.0.3 checkout (needs {', '.join(needed)})"
    return None


def parse_args():
    ap = argparse.ArgumentParser()
    ap.add_argument("slices", nargs="*")
    ap.add_argument("--jobs", type=int, default=MAX_JOBS)
    ap.add_argument("--batch-size", type=int, default=60)
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


def report_control(by, ctl, ref_dir, done, verbose):
    cn, cj = merge([by[("node", "ctl")]]), merge([by[("jsse", "ctl")]])
    print(
        f"control ({'; '.join(done)}): Node fails {counts(cn)['failing']} of {sum(counts(cn).values())}, jsse fails {counts(cj)['failing']}"
    )
    if verbose:
        for t, m in sorted(cn["errors"].items()):
            print(f"control failure: {t}\n    " + m[:400].replace("\n", "\n    "))
    cp = [
        f"control: {p}"
        for p in classify(cn, cj, set())
        if not p.startswith("fails on Node itself")
    ]
    cp += [f"control: {p}" for p in local_problems(cn, cj, ref_dir)]
    cp += [f"control: {b}" for b in cn["bad"] + cj["bad"]]
    failed = {t for (t, k) in cn["outcomes"] if k == "failing"}
    cp += [
        f"control: no test of {s.key} failed on Node (the corrupted reference was not noticed)"
        for s in ctl
        if not failed.intersection(s.tests)
    ]
    return cp


def main():
    o = parse_args()
    table = dict(ps.SLICES)
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
    bad = checkout_problem(ts_dir)
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
    work = os.path.realpath(tempfile.mkdtemp(prefix="ts-project-oracle-"))
    jobs = max(1, min(o.jobs, MAX_JOBS))
    spec = "+".join(o.slices) if o.slices else "matrix"
    prep = lambda tag, ref=None: compiler.prepare(work, tag, ts_dir, ref, CONFIG)  # noqa: E731
    try:
        listing = compiler.mocha_run(
            "node", node, prep("list"), "^projects tests ", 60000, 600, dry=True
        )
        if listing["report"] is None:
            print("cannot list the project tests:\n" + listing["tail"])
            return 1
        everything = ps.suites_from_titles(
            (t["fullTitle"], t["title"]) for t in listing["report"].get("tests", [])
        )
        try:
            chosen = ps.select(spec, everything, table)
        except (ValueError, re.error) as e:
            sys.exit(str(e))
        total = sum(len(s.tests) for s in chosen)
        files = len({s.file for s in chosen})
        print(
            f"project {spec}: {len(chosen)} of {len(everything)} suites ({files} json files, {total} tests)",
            flush=True,
        )
        if o.list:
            print("\n".join(s.key for s in chosen))
            return 0
        bs = ps.batches(chosen, max(1, o.batch_size))
        print(
            f"{len(bs)} batches of up to {o.batch_size} suites, {jobs} jobs, jsse: {jsse}",
            flush=True,
        )

        def one(task):
            engine, tag, batch, ref_dir = task
            exe = node if engine == "node" else jsse
            r = compiler.mocha_run(
                engine,
                exe,
                prep(f"{tag}-{engine}", ref_dir),
                ps.grep_for(batch),
                o.test_timeout,
                o.timeout,
            )
            print(f"done {tag} {engine} {r['secs']:.0f}s", flush=True)
            return r

        engines = ["jsse", "node"] if not o.no_jsse else ["node"]
        tasks = [(e, f"b{i}", b, None) for e in engines for i, b in enumerate(bs)]
        ctl, done, ref_dir = [], [], None
        if not o.no_control and not o.no_jsse and chosen:
            ref_dir = os.path.join(work, "control-ref")
            os.makedirs(ref_dir)
            ctl, done = control.make_control(
                os.path.join(ts_dir, REF),
                chosen + [s for s in everything if s not in chosen],
                ref_dir,
            )
            tasks += [(e, "ctl", ctl, ref_dir) for e in engines]
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
        in_slice = {x for s in chosen for x in s.tests}
        mine = {t: w for t, w in known.items() if t in in_slice}
        nc = counts(node_res)
        print(
            f"\nNode: {nc['passing']} passing, {nc['failing']} failing, {nc['pending']} pending in {node_res['secs']:.0f}s"
        )
        problems = [f"Node: {b}" for b in node_res["bad"]]
        if sum(nc.values()) != total:
            problems.append(f"Node ran {sum(nc.values())} tests, the slice has {total}")
        want = pinned_expected(chosen, everything)
        if want is not None and nc["passing"] != want:
            problems.append(
                f"ts-suite/oracle.json pins {want} passing project tests, Node ran {nc['passing']}"
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
            problems += report_control(by, ctl, ref_dir, done, o.verbose)
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
            write_known(o.known, new, "run-ts-project-oracle.py")
            print(f"wrote {o.known}: {len(new)} known failures")
            return 0
        for p in problems:
            print("  " + p)
        print("project oracle: " + ("FAIL" if problems else "PASS"))
        return 1 if problems else 0
    finally:
        if o.keep_work:
            shutil.copytree(work, o.keep_work, dirs_exist_ok=True, symlinks=True)
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
