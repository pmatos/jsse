#!/usr/bin/env python3
"""Run slices of TypeScript's `compiler` runner on Node and on jsse --node and diff them per test (issue #1268).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-compiler-oracle.py [options] [slice ...]

TS_SRC_DIR is a TypeScript v6.0.3 checkout with `npm ci` and `hereby local`/`hereby tests` done (node_modules/mocha,
built/local/run.js); without it the script prints SKIP and exits 0 unless --require is given. It never writes into the
checkout: every run gets a private working directory of symlinks into it (tests/cases, tests/lib, the reference
baselines) and its own empty tests/baselines/local, so the baseline files a run writes are the run's own.

A slice (scripts/ts_compiler_slices.py: `first:N`, `prefix:0-9,A-C`, `shard:I/N`, `files:a.ts`, `match:RE`, a named
row of SLICES, or a '+' union; default first200) selects suites of tests/cases/compiler (a test file, or one
configuration of it, six tests each) in the runner's enumeration order, taken from a Node mocha dry run. The suites
are cut into batches (--batch-size) and every batch is one mocha process per engine (jsse --node and Node, at most 4
processes at a time over both engines), because the first Program of a process costs 25 to 75 s on jsse (lib.d.ts)
and the 13 MB runner bundle 80 s to load. A test passes iff its baselines equal tests/baselines/reference; a
mismatch writes the actual text to tests/baselines/local and fails with the diff in the test's error. Compared:

  * the outcome of every test (the JSON reporter's fullTitle) between the engines;
  * for a test failing on both, the error message (the baseline diff text) and
  * every file under tests/baselines/local, byte for byte, each of which must differ from its reference.

Node must pass everything (ts-suite/oracle.json pins 43656 passing compiler tests); a test passing on Node and failing
on jsse must be listed in ts-suite/compiler-known-failures.txt (`full title<TAB>issue`), a listed test that passes
fails the run (ratchet). With --control (default on) a few tests of the slice run again against a deliberately
corrupted private copy of their reference baselines (changed, appended-to and missing files); the engines must fail the
same tests with the same messages and write the same local files, which checks the diff and baseline-writing path.

Options: --jobs N (default 4, at most 4), --batch-size N (default 40 suites), --test-timeout MS (mocha -t, default
600000), --timeout SECONDS (per process, default 3600), --list, --require, --no-jsse, --no-control, --record (rewrite
the known failures of the slice), --jsse PATH (else $JSSE, else target/release/jsse), --known FILE,
--slices-module NAME (extra SLICES rows from scripts/NAME.py), --keep-work DIR, --verbose.
"""

import argparse
import importlib
import json
import os
import shutil
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import oracle_procs  # noqa: E402
import ts_compiler_control  # noqa: E402
import ts_compiler_slices as slices  # noqa: E402
from ts_compiler_report import (  # noqa: E402,F401
    classify,
    counts,
    load_known,
    local_problems,
    merge,
    outcomes_of,
    recordable,
    write_known,
)

KNOWN = os.path.join(ROOT, "ts-suite", "compiler-known-failures.txt")
SUITES = os.path.join(ROOT, "ts-suite", "oracle.json")
DROP_ENV = (
    "NODE_OPTIONS",
    "FORCE_COLOR",
    "NO_COLOR",
    "COLUMNS",
    "NODE_DISABLE_COLORS",
    "CI",
)
LINKS = ("built", "node_modules", "scripts", "lib", "src", "package.json")
MAX_JOBS = 4
CONFIG = {"runners": ["compiler"], "light": False}
LOCAL = os.path.join("tests", "baselines", "local")
REF = os.path.join("tests", "baselines", "reference")
SUITE_GREP = "^compiler tests compiler tests for "


def prepare(work, tag, ts_dir, ref_dir=None, config=None):
    cwd = os.path.join(work, tag)
    os.makedirs(os.path.join(cwd, LOCAL))
    os.makedirs(os.path.join(cwd, "tmp"))
    for link in LINKS:
        os.symlink(os.path.join(ts_dir, link), os.path.join(cwd, link))
    for link in ("cases", "lib"):
        os.symlink(
            os.path.join(ts_dir, "tests", link), os.path.join(cwd, "tests", link)
        )
    os.symlink(ref_dir or os.path.join(ts_dir, REF), os.path.join(cwd, REF))
    with open(os.path.join(cwd, "test.config"), "w") as f:
        json.dump(config or CONFIG, f)
    return cwd


def mocha_run(engine, exe, cwd, grep, test_timeout, timeout, dry=False):
    env = {k: v for k, v in os.environ.items() if k not in DROP_ENV}
    env["NODE_ENV"] = "development"
    env["TMPDIR"] = os.path.join(cwd, "tmp")
    with open(os.path.join(cwd, "grep.json"), "w") as f:
        json.dump({"grep": grep}, f)
    cmd = [exe] + (["--node"] if engine == "jsse" else [])
    cmd += [os.path.join(cwd, "node_modules", "mocha", "bin", "_mocha"), "--no-colors"]
    cmd += ["-t", str(test_timeout), "-R", "json", "-O", "output=report.json"]
    cmd += (
        ["--config", "grep.json"]
        + (["--dry-run"] if dry else [])
        + ["built/local/run.js"]
    )
    start = time.monotonic()
    code, out, err = oracle_procs.run_group(cmd, cwd, env, timeout)
    res = {"exit": code, "secs": time.monotonic() - start, "report": None, "local": {}}
    text = (out + err).decode("utf-8", "replace")
    res["tail"] = text if len(text) < 2500 else text[:800] + "\n[...]\n" + text[-1500:]
    try:
        with open(os.path.join(cwd, "report.json"), encoding="utf-8") as f:
            res["report"] = json.load(f)
    except (OSError, ValueError):
        pass
    for dirpath, _, files in os.walk(os.path.join(cwd, LOCAL)):
        for name in files:
            path = os.path.join(dirpath, name)
            with open(path, "rb") as f:
                res["local"][os.path.relpath(path, os.path.join(cwd, LOCAL))] = f.read()
    return res


def pinned_expected(spec, table, runner="compiler"):
    """Passing tests ts-suite/oracle.json pins for a spec of prefix terms only, else None."""
    terms = slices.flat_terms(spec, table)
    if not terms or not all(t.startswith("prefix:") for t in terms):
        return None
    try:
        with open(SUITES, encoding="utf-8") as f:
            buckets = json.load(f)["runners"][runner]["buckets"]
    except (OSError, ValueError, KeyError):
        return None
    preds = [slices.char_ranges(t[7:]) for t in terms]
    return sum(v["passed"] for k, v in buckets.items() if any(p(k) for p in preds))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("slices", nargs="*")
    ap.add_argument("--jobs", type=int, default=MAX_JOBS)
    ap.add_argument("--batch-size", type=int, default=40)
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
    o = ap.parse_args()
    table = dict(slices.SLICES)
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
    try:
        with open(os.path.join(ts_dir, "package.json"), encoding="utf-8") as f:
            version = json.load(f).get("version")
    except (OSError, ValueError):
        version = None
    needed = (
        "node_modules/mocha/bin/_mocha",
        "built/local/run.js",
        "tests/cases/compiler",
    )
    if version != "6.0.3" or not all(
        os.path.exists(os.path.join(ts_dir, p)) for p in needed
    ):
        return skip(
            f"{ts_dir} is not a built TypeScript 6.0.3 checkout (needs {', '.join(needed)})"
        )
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
    work = os.path.realpath(tempfile.mkdtemp(prefix="ts-compiler-oracle-"))
    jobs = max(1, min(o.jobs, MAX_JOBS))
    spec = "+".join(o.slices) if o.slices else "first200"
    try:
        listing = mocha_run(
            "node",
            node,
            prepare(work, "list", ts_dir),
            SUITE_GREP,
            60000,
            600,
            dry=True,
        )
        if listing["report"] is None:
            print("cannot list the compiler tests:\n" + listing["tail"])
            return 1
        everything = slices.suites_from_titles(
            (t["fullTitle"], t["title"]) for t in listing["report"].get("tests", [])
        )
        try:
            chosen = slices.select(spec, everything, table)
        except ValueError as e:
            sys.exit(str(e))
        print(
            f"{spec}: {len(chosen)} of {len(everything)} suites ({sum(len(s.tests) for s in chosen)} tests)",
            flush=True,
        )
        if o.list:
            print("\n".join(s.key[len(slices.PREFIX) :] for s in chosen))
            return 0
        bs = slices.batches(chosen, o.batch_size)
        print(
            f"{len(bs)} batches of up to {o.batch_size} suites, {jobs} jobs, jsse: {jsse}",
            flush=True,
        )

        def one(task):
            engine, tag, batch, ref_dir = task
            cwd = prepare(work, f"{tag}-{engine}", ts_dir, ref_dir)
            exe = node if engine == "node" else jsse
            r = mocha_run(
                engine, exe, cwd, slices.grep_for(batch), o.test_timeout, o.timeout
            )
            print(f"done {tag} {engine} {r['secs']:.0f}s", flush=True)
            return r

        engines = ["jsse", "node"] if not o.no_jsse else ["node"]
        tasks = [(e, f"b{i}", b, None) for e in engines for i, b in enumerate(bs)]
        control, done = [], []
        if not o.no_control and not o.no_jsse and chosen:
            cref = os.path.join(work, "control-ref")
            os.makedirs(cref)
            control, done = ts_compiler_control.make_control(
                os.path.join(ts_dir, REF), chosen, cref
            )
            tasks += [(e, "ctl", control, cref) for e in engines]
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
        in_slice = {t for s in chosen for t in s.tests}
        mine = {t: w for t, w in known.items() if t in in_slice}
        nc = counts(node_res)
        print(
            f"\nNode: {nc['passing']} passing, {nc['failing']} failing, {nc['pending']} pending in {node_res['secs']:.0f}s"
        )
        problems = [f"Node: {b}" for b in node_res["bad"]]
        expected = sum(len(s.tests) for s in chosen)
        if sum(nc.values()) != expected:
            problems.append(
                f"Node ran {sum(nc.values())} tests, the slice has {expected}"
            )
        want = pinned_expected(spec, table)
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
            if sum(jc.values()) != expected:
                problems.append(
                    f"jsse ran {sum(jc.values())} tests, the slice has {expected}"
                )
            problems += classify(node_res, jsse_res, mine)
            problems += local_problems(node_res, jsse_res, os.path.join(ts_dir, REF))
            if o.verbose:
                print("\n".join(f"known failure: {t} ({w})" for t, w in mine.items()))
        if control and jsse_res is not None:
            cn, cj = merge([by[("node", "ctl")]]), merge([by[("jsse", "ctl")]])
            print(
                f"control ({', '.join(s.base for s in control)}; {'; '.join(done)}): Node fails {counts(cn)['failing']}, jsse fails {counts(cj)['failing']}"
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
            cp += [
                f"control: {p}"
                for p in local_problems(cn, cj, os.path.join(work, "control-ref"))
            ]
            cp += [f"control: {b}" for b in cn["bad"] + cj["bad"]]
            if not counts(cn)["failing"] or not cn["local"]:
                cp.append(
                    "control: the corrupted references made no Node test fail (the control checks nothing)"
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
            write_known(o.known, new)
            print(f"wrote {o.known}: {len(new)} known failures")
            return 0
        for p in problems:
            print("  " + p)
        print("compiler oracle: " + ("FAIL" if problems else "PASS"))
        return 1 if problems else 0
    finally:
        if o.keep_work:
            shutil.copytree(work, o.keep_work, dirs_exist_ok=True, symlinks=True)
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
