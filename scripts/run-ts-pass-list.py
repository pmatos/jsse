#!/usr/bin/env python3
"""Run TypeScript's own test suite on jsse --node and diff it against the checked-in pass list (issue #1296).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-pass-list.py [options] [runner[:bucket,...] ...]

Runners: compiler conformance project transpile fourslash fourslash-server unittests (default: all). A bucket is the
first letter of the test file (compiler, fourslash, fourslash-server), the directory (conformance), the unit-test
suite (unittests), the file-name family (project) or `all` (transpile); `--list` prints them. TS_SRC_DIR is a built
TypeScript v6.0.3 checkout (as for scripts/run-ts-compiler-oracle.py); without it the script prints SKIP and exits 0
unless --require is given. The checkout is never written to: every batch runs in a private directory of symlinks.

The test universe comes from a Node mocha dry run of each runner and is checked against ts-suite/oracle.json (the
sha256 of the passing ids per runner). The pass list is ts-suite/jsse-pass-1296.json (recorded buckets: tests and
sha256 of their ids) minus ts-suite/pass-list-1296-not-passing.txt (scripts/ts_passlist_store.py); like
scripts/run-test262.py it is read from --baseline-ref (default origin/main) with a fallback to the working tree. The
selected suites run on jsse in batches (one mocha process each, at most 4 at a time) and every test lands in one of
pass, fail, new-pass (passes, not in the pass list) or regression (in the pass list, failed or did not report). The
table per runner is printed with a JSON summary line; exit status 1 on a regression, a universe or bucket that
differs from the pin, a batch without a report, or an empty selection.

Options: --engine {jsse,node} (node re-runs the selection on Node: the cross-check, 0 regressions expected), --jsse
PATH (else $JSSE, else target/release/jsse), --jobs N (default 4, at most 4), --batch-size N (tests per mocha
process, default 240), --test-timeout MS (default 600000), --timeout SECONDS (per process, default 7200), --sample
RATE --seed N (a fraction of the suites per runner), --baseline-ref REF, --check-universe (dry runs only), --list,
--require, --update-baseline [--issue TEXT], --keep-work DIR, --verbose.

--update-baseline records every bucket this run covered completely and without a failure, and removes the
not-passing entries that now pass, in the working-tree files; it never drops an entry and refuses to touch anything
when there is a regression or a failed batch. A completed bucket with failing tests needs --issue (the issue that
tracks them); they are written to the not-passing file.
"""

import argparse
import collections
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor

SCRIPTS = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(SCRIPTS)
sys.path.insert(0, SCRIPTS)
import oracle_procs  # noqa: E402
import ts_passlist as pl  # noqa: E402
import ts_passlist_scripted as scripted  # noqa: E402
import ts_passlist_store as store  # noqa: E402
from ts_compiler_report import outcomes_of  # noqa: E402

_spec = importlib.util.spec_from_file_location(
    "run_ts_compiler_oracle", os.path.join(SCRIPTS, "run-ts-compiler-oracle.py")
)
compiler = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(compiler)

MAX_JOBS = 4
NEEDED = ("node_modules/mocha/bin/_mocha", "built/local/run.js", "tests/cases/compiler")
DATA_DIR = os.path.join(ROOT, "ts-suite")
ORACLE_JSON = os.path.join(DATA_DIR, "oracle.json")


def data_path(data_dir, rel):
    return os.path.join(data_dir, os.path.basename(rel))


def load_baseline(ref, data_dir=DATA_DIR):
    """(doc, not-passing dict, source) from `ref` (default data directory only), else from the files of the
    data directory, else an empty baseline."""
    sources = [("working tree", lambda p: read_file(data_path(data_dir, p)))]
    if ref and data_dir == DATA_DIR:
        sources.insert(0, (ref, lambda p: store.read_at(ref, p, cwd=ROOT)))
    for source, read in sources:
        text = read(store.BASELINE)
        if text is not None:
            np_text = read(store.NOT_PASSING)
            return (
                store.parse_baseline(text),
                store.parse_not_passing(np_text or ""),
                source,
            )
    with open(os.path.join(ROOT, "ts-suite", "pin.json"), encoding="utf-8") as f:
        return store.empty_baseline(json.load(f)), {}, "none"


def read_file(path):
    try:
        with open(path, encoding="utf-8") as f:
            return f.read()
    except OSError:
        return None


def mocha_config(runner):
    return {"runners": [pl.MOCHA_RUNNER.get(runner, runner)], "light": False}


def list_universe(work, ts_dir, node, runners):
    """{runner: [Test]} from a Node mocha dry run per runner; None for a runner that could not be listed."""
    out = {}
    for runner in runners:
        if runner in scripted.SCRIPTED:
            out[runner] = scripted.list_cases(
                runner, scripted.python_exe(), dict(os.environ)
            )
            continue
        cwd = compiler.prepare(
            work, f"list-{runner}", ts_dir, None, mocha_config(runner)
        )
        res = compiler.mocha_run("node", node, cwd, "", 60000, 900, dry=True)
        if res["report"] is None:
            print(f"cannot list the {runner} tests:\n" + res["tail"])
            out[runner] = None
            continue
        tests = pl.tests_from_titles(
            (t["fullTitle"], t["title"]) for t in res["report"].get("tests", [])
        )
        out[runner] = [t for t in tests if t.runner == runner]
    return out


def universe_problems(universe, oracle_json=ORACLE_JSON):
    """Differences between the dry-run universe of each runner and the passing set ts-suite/oracle.json pins."""
    with open(oracle_json, encoding="utf-8") as f:
        pinned = json.load(f)["runners"]
    problems = []
    for runner, tests in universe.items():
        if runner in scripted.SCRIPTED:
            continue
        want = pinned[runner]
        if tests is None:
            problems.append(f"{runner}: could not be listed")
        elif (len(tests), pl.digest_of(tests)) != (want["passed"], want["digest"]):
            problems.append(
                f"{runner}: {len(tests)} tests listed, ts-suite/oracle.json pins {want['passed']} passing ids with another digest"
            )
    return problems


def bucket_problems(doc, universe):
    """Recorded buckets whose ids differ from the universe (the baseline is for another test set)."""
    problems = []
    for runner, tests in universe.items():
        by = collections.defaultdict(list)
        for t in tests or []:
            by[t.bucket].append(t)
        for name, e in doc["buckets"].get(runner, {}).items():
            got = by.get(name, [])
            if [len(got), store.bucket_digest(t.id for t in got)] != e:
                problems.append(
                    f"recorded bucket {runner}:{name} has {e[0]} tests, the checkout lists {len(got)} (or other ids)"
                )
    return problems


def run_batches(args, work, ts_dir, exe, chosen):
    """Run every batch; returns ({runner: outcome table}, [errors])."""
    tasks = []
    for runner, suites in chosen.items():
        if runner in scripted.SCRIPTED:
            continue
        for i, b in enumerate(pl.batches(suites, args.batch_size)):
            tasks.append((runner, f"{runner}-{i}", b))

    def one(task):
        runner, tag, batch = task
        cwd = compiler.prepare(work, tag, ts_dir, None, mocha_config(runner))
        res = compiler.mocha_run(
            args.engine, exe, cwd, pl.grep_for(batch), args.test_timeout, args.timeout
        )
        n = sum(len(ts) for _, ts in batch)
        print(f"done {tag} ({n} tests) {res['secs']:.0f}s", flush=True)
        res["local"] = {}
        shutil.rmtree(cwd, ignore_errors=True)
        return res

    with ThreadPoolExecutor(max(1, min(args.jobs, MAX_JOBS))) as pool:
        results = list(pool.map(one, tasks))
    tables = {r: collections.Counter() for r in chosen}
    errors = []
    for runner, suites in chosen.items():
        if runner in scripted.SCRIPTED:
            cases = [t for _, ts in suites for t in ts]
            outcomes, error = scripted.run_cases(
                runner,
                cases,
                scripted.python_exe(),
                exe,
                max(1, min(args.jobs, MAX_JOBS)),
                args.timeout,
                dict(os.environ),
            )
            tables[runner].update(outcomes)
            errors += [error] if error else []
            print(f"done {runner} ({len(cases)} cases)", flush=True)
    for (runner, tag, _), res in zip(tasks, results):
        if res["report"] is None:
            errors.append(
                f"{tag}: exit {res['exit']}, no report; output tail:\n{res['tail']}"
            )
        tables[runner].update(outcomes_of(res["report"])[0])
    return {r: pl.outcome_table(c) for r, c in tables.items()}, errors


def apply_update(doc, np, run, complete, args, jsse_rev):
    """Record the complete buckets of a clean run: (new doc, new not-passing dict, notes)."""
    doc = json.loads(json.dumps(doc))
    np = dict(np)
    notes = []
    for (runner, name), tests in sorted(complete.items()):
        failing = [t for t in tests if t.id in run["failing_ids"]]
        table = doc["buckets"].setdefault(runner, {})
        if name in table:
            for key in [k for k in np if k[:2] == (runner, name)]:
                if key[2] in run["passing_ids"]:
                    del np[key]
                    notes.append(f"{runner}:{name}: no longer failing: {key[2][:100]}")
            continue
        if failing and not args.issue:
            notes.append(
                f"{runner}:{name}: not recorded, {len(failing)} tests fail (pass --issue to list them)"
            )
            continue
        table[name] = [len(tests), store.bucket_digest(t.id for t in tests)]
        doc["jsse"] = jsse_rev
        for t in failing:
            np[(runner, name, t.id)] = args.issue
        notes.append(
            f"{runner}:{name}: recorded ({len(tests)} tests, {len(failing)} failing)"
        )
    return doc, np, notes


def jsse_revision():
    r = subprocess.run(
        ["git", "-C", ROOT, "rev-parse", "--short=10", "HEAD"],
        capture_output=True,
        text=True,
    )
    return r.stdout.strip() or "unknown"


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("specs", nargs="*", metavar="runner[:bucket,...]")
    ap.add_argument("--engine", choices=("jsse", "node"), default="jsse")
    ap.add_argument("--jsse")
    ap.add_argument("--jobs", type=int, default=MAX_JOBS)
    ap.add_argument("--batch-size", type=int, default=240)
    ap.add_argument("--test-timeout", type=int, default=600000)
    ap.add_argument("--timeout", type=int, default=7200)
    ap.add_argument("--sample", type=float)
    ap.add_argument("--seed", type=int)
    ap.add_argument("--baseline-ref", default="origin/main")
    ap.add_argument("--issue")
    ap.add_argument("--keep-work")
    ap.add_argument(
        "--data-dir", default=DATA_DIR, help="directory of the pass list files"
    )
    ap.add_argument(
        "--oracle-json", default=ORACLE_JSON, help="the manifest that pins the universe"
    )
    for flag in (
        "--check-universe",
        "--list",
        "--require",
        "--update-baseline",
        "--verbose",
    ):
        ap.add_argument(flag, action="store_true")
    args = ap.parse_args(argv)
    try:
        for spec in args.specs:
            pl.parse_spec(spec)
    except ValueError as e:
        ap.error(str(e))
    if args.update_baseline and args.engine != "jsse":
        ap.error("--update-baseline records the jsse pass list; use --engine jsse")

    def skip(msg):
        print(("ERROR: " if args.require else "SKIP: ") + msg)
        return 3 if args.require else 0

    if not os.environ.get("TS_SRC_DIR"):
        return skip(
            "TS_SRC_DIR is not set; point it at a built TypeScript v6.0.3 checkout"
        )
    ts_dir = os.path.realpath(os.environ["TS_SRC_DIR"])
    pkg = read_file(os.path.join(ts_dir, "package.json"))
    if (
        not pkg
        or json.loads(pkg).get("version") != "6.0.3"
        or not all(os.path.exists(os.path.join(ts_dir, p)) for p in NEEDED)
    ):
        return skip(
            f"{ts_dir} is not a built TypeScript 6.0.3 checkout (needs {', '.join(NEEDED)})"
        )
    node = shutil.which("node")
    if not node:
        return skip("node not found")
    jsse = os.path.abspath(
        args.jsse
        or os.environ.get("JSSE")
        or os.path.join(ROOT, "target", "release", "jsse")
    )
    exe = node if args.engine == "node" else jsse
    if (
        args.engine == "jsse"
        and not (args.list or args.check_universe)
        and not os.access(jsse, os.X_OK)
    ):
        print(f"jsse binary not found or not executable: {jsse}", file=sys.stderr)
        return 1

    oracle_procs.install_cleanup()
    work = os.path.realpath(tempfile.mkdtemp(prefix="ts-pass-list-"))
    try:
        runners = [pl.parse_spec(s)[0] for s in args.specs]
        for runner in runners:
            why = None
            if runner in scripted.SCRIPTED:
                why = (
                    "the scripted oracles compare jsse with Node, so they have no Node-only run"
                    if args.engine == "node"
                    else scripted.unavailable(runner)
                )
            if why:
                return skip(f"{runner}: {why}")
        if not runners:
            runners = list(pl.RUNNERS)
            if args.engine == "jsse":
                runners += [r for r in scripted.SCRIPTED if not scripted.unavailable(r)]
            args.specs = list(runners)
        universe = list_universe(work, ts_dir, node, list(dict.fromkeys(runners)))
        problems = universe_problems(universe, args.oracle_json)
        doc, np, source = load_baseline(args.baseline_ref, args.data_dir)
        problems += bucket_problems(doc, universe)
        chosen = pl.select(universe, args.specs, args.sample, args.seed)
        total = sum(len(ts) for s in chosen.values() for _, ts in s)
        print(
            f"baseline: {source}; universe: "
            + ", ".join(f"{r} {len(t or [])}" for r, t in universe.items())
        )
        print(
            f"selected {total} tests in {sum(len(s) for s in chosen.values())} suites ({args.engine})",
            flush=True,
        )
        if args.list:
            for runner, suites in chosen.items():
                by = collections.Counter()
                for _, ts in suites:
                    by[ts[0].bucket] += len(ts)
                rec = doc["buckets"].get(runner, {})
                print(
                    f"{runner}: "
                    + ", ".join(
                        f"{b}={n}{'*' if b in rec else ''}"
                        for b, n in sorted(by.items())
                    )
                )
            print("(* = recorded in the pass list)")
            return 0
        if args.check_universe:
            print("\n".join("  " + p for p in problems))
            print("pass list universe: " + ("FAIL" if problems else "PASS"))
            return 1 if problems else 0
        if not total:
            print("nothing selected")
            return 1

        tables, errors = run_batches(args, work, ts_dir, exe, chosen)
        problems += errors
        recorded = {(r, b) for r, t in doc["buckets"].items() for b in t}
        held = {k[2] for k in np}
        rows, run = {}, {"passing_ids": set(), "failing_ids": set()}
        for runner, suites in chosen.items():
            selected = [t for _, ts in suites for t in ts]
            res = pl.compare(
                selected,
                tables[runner],
                lambda t: (t.runner, t.bucket) in recorded and t.id not in held,
            )
            rows[runner] = res
            run["passing_ids"].update(t.id for t in res["pass"])
            run["failing_ids"].update(t.id for t in res["fail"] + res["regression"])
        report(rows, args, problems)
        regressions = sum(len(r["regression"]) for r in rows.values())
        if regressions:
            problems.append(f"{regressions} baseline regression(s) detected")
        if args.update_baseline:
            if regressions or errors or problems:
                print("not updating the baseline: the run has regressions or problems")
            else:
                update(args, chosen, universe, run)
        print(
            "".join("  " + p + "\n" for p in problems)
            + ("pass list: FAIL" if problems else "pass list: PASS")
        )
        return 1 if problems else 0
    finally:
        if args.keep_work:
            shutil.copytree(work, args.keep_work, dirs_exist_ok=True, symlinks=True)
        shutil.rmtree(work, ignore_errors=True)


def update(args, chosen, universe, run):
    doc, np, _ = load_baseline(None, args.data_dir)
    complete = {}
    for runner, suites in chosen.items():
        have = collections.defaultdict(list)
        for _, ts in suites:
            have[ts[0].bucket].extend(ts)
        for name, tests in have.items():
            full = [t for t in universe[runner] if t.bucket == name]
            if len(full) == len(tests):
                complete[(runner, name)] = tests
    new_doc, new_np, notes = apply_update(doc, np, run, complete, args, jsse_revision())
    print("\n".join(notes))
    if new_doc != doc or new_np != np:
        for rel, text in (
            (store.BASELINE, store.dump_baseline(new_doc)),
            (store.NOT_PASSING, store.dump_not_passing(new_np)),
        ):
            with open(data_path(args.data_dir, rel), "w", encoding="utf-8") as f:
                f.write(text)
        print(f"wrote {store.BASELINE} and {store.NOT_PASSING}")


def report(rows, args, problems):
    print(f"\n=== TypeScript suite on {args.engine} ===")
    print(
        f"{'runner':<18}{'baseline':>9}{'pass':>8}{'fail':>7}{'new-pass':>10}{'regression':>12}"
    )
    sums = collections.Counter()
    for runner, r in rows.items():
        line = (
            r["held"],
            len(r["pass"]),
            len(r["fail"]) + len(r["regression"]),
            len(r["new_pass"]),
            len(r["regression"]),
        )
        sums.update(dict(enumerate(line)))
        print(
            f"{runner:<18}{line[0]:>9}{line[1]:>8}{line[2]:>7}{line[3]:>10}{line[4]:>12}"
        )
        for t in r["regression"][:20]:
            print(f"  REGRESSED: {t.id[:200]}")
        if args.verbose:
            for t in r["fail"][:20]:
                print(f"  FAILED: {t.id[:200]}")
    t = [sums[i] for i in range(5)]
    print(f"{'total':<18}{t[0]:>9}{t[1]:>8}{t[2]:>7}{t[3]:>10}{t[4]:>12}")
    summary = dict(zip(("baseline", "pass", "fail", "new_pass", "regression"), t))
    print(
        "JSON: "
        + json.dumps({"engine": args.engine, **summary, "problems": len(problems)})
    )


if __name__ == "__main__":
    sys.exit(main())
