#!/usr/bin/env python3
"""Wall-time and peak-RSS budget of TypeScript's own test suite on jsse --node, against Node (issue #1297).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-budget-oracle.py [options] [case ...]

TS_SRC_DIR is a TypeScript v6.0.3 checkout with `npm ci` and `hereby tests` done (node_modules/mocha, built/local/run.js);
without it the script prints SKIP and exits 0 unless --require is given. The checkout is never written to: every run
gets a private working directory of symlinks into it with its own empty tests/baselines/local.

Each case (scripts/ts_budget_cases.py) is one mocha process per engine over built/local/run.js: the runners that
test.config enables plus a --grep over the full test titles. The two engines run one after the other so that wall time
is not shared with the other engine; `--jobs N` (at most 4) runs that many cases at once, which leaves peak RSS alone
and stretches wall time. Compared per case: every test passing on Node must pass on jsse (or be listed in
ts-suite/budget-1297-known-failures.txt, `full title<TAB>issue`; a listed test that passes fails the run).
Measured per run: wall time and peak RSS (`ru_maxrss` of the largest process of the tree). Checked against
ts-suite/budget-1297.json (per case: max_rss_ratio, the shipped cap of 6x Node; jsse_max_rss_mb, jsse_max_seconds and
max_time_ratio exist for a local override but are null and rejected by --check-budget in the shipped file: the ruling
on #1297 records wall time and does not cap it). Exits 1 on any outcome difference or exceeded budget.

--smoke runs the CI subset (cases marked smoke). --check-budget validates the budget file against the case table and
needs no checkout. --list prints the cases. Options: --jsse PATH (else $JSSE, else target/release/jsse), --test-timeout
MS (mocha -t, default 600000), --timeout SECONDS (per process, default 7200), --budget FILE, --known FILE, --report
FILE (JSON of every measurement), --keep-work DIR.
"""

import argparse
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
import ts_budget_cases as cases_table  # noqa: E402
import ts_budget_check as check  # noqa: E402
from ts_compiler_report import load_known  # noqa: E402

_spec = importlib.util.spec_from_file_location(
    "run_ts_compiler_oracle", os.path.join(SCRIPTS, "run-ts-compiler-oracle.py")
)
compiler = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(compiler)

BUDGET = os.path.join(ROOT, "ts-suite", "budget-1297.json")
KNOWN = os.path.join(ROOT, "ts-suite", "budget-1297-known-failures.txt")
MAX_JOBS = 4


def mocha_command(engine, exe, cwd, grep, test_timeout):
    with open(os.path.join(cwd, "grep.json"), "w") as f:
        json.dump({"grep": grep}, f)
    cmd = [exe] + (["--node"] if engine == "jsse" else [])
    cmd += [os.path.join(cwd, "node_modules", "mocha", "bin", "_mocha"), "--no-colors"]
    cmd += ["-t", str(test_timeout), "-R", "json", "-O", "output=report.json"]
    return cmd + ["--config", "grep.json", "built/local/run.js"]


def run_case(engine, exe, name, ts_dir, work, test_timeout, timeout):
    runners, grep, _, _ = cases_table.CASES[name]
    cwd = compiler.prepare(
        work, f"{name}-{engine}", ts_dir, None, {"runners": runners, "light": False}
    )
    env = {k: v for k, v in os.environ.items() if k not in compiler.DROP_ENV}
    env["NODE_ENV"] = "development"
    env["TMPDIR"] = os.path.join(cwd, "tmp")
    code, out, secs, rss = check.run_measured(
        mocha_command(engine, exe, cwd, grep, test_timeout), cwd, env, timeout
    )
    report = None
    try:
        with open(os.path.join(cwd, "report.json"), encoding="utf-8") as f:
            report = json.load(f)
    except (OSError, ValueError):
        pass
    local = []
    for dirpath, _, files in os.walk(os.path.join(cwd, compiler.LOCAL)):
        local += [os.path.join(dirpath, n) for n in files]
    text = out.decode("utf-8", "replace")
    return {
        "exit": code,
        "seconds": secs,
        "rss_mb": rss,
        "summary": check.summarise(report),
        "local": local,
        "tail": text if len(text) < 2500 else text[:800] + "\n[...]\n" + text[-1500:],
    }


def public(run):
    s = run["summary"]
    return {
        "seconds": round(run["seconds"], 1),
        "rss_mb": round(run["rss_mb"], 1),
        "exit": run["exit"],
        "passed": len(s[0]) if s else None,
        "failed": len(s[1]) if s else None,
    }


def inputs(ts_dir):
    needed = (
        "node_modules/mocha/bin/_mocha",
        "built/local/run.js",
        "tests/cases/compiler",
    )
    try:
        with open(os.path.join(ts_dir, "package.json"), encoding="utf-8") as f:
            version = json.load(f).get("version")
    except (OSError, ValueError):
        version = None
    if version != "6.0.3" or not all(
        os.path.exists(os.path.join(ts_dir, p)) for p in needed
    ):
        return f"{ts_dir} is not a built TypeScript 6.0.3 checkout (needs {', '.join(needed)})"
    return None


def main():
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("cases", nargs="*")
    ap.add_argument("--jsse")
    ap.add_argument("--jobs", type=int, default=1)
    ap.add_argument("--test-timeout", type=int, default=600000)
    ap.add_argument("--timeout", type=int, default=7200)
    ap.add_argument("--budget", default=BUDGET)
    ap.add_argument("--known", default=KNOWN)
    ap.add_argument("--report")
    ap.add_argument("--keep-work")
    for flag in ("--smoke", "--list", "--check-budget", "--require"):
        ap.add_argument(flag, action="store_true")
    o = ap.parse_args()
    table = cases_table.CASES
    if o.list:
        for name, (_, grep, smoke, why) in table.items():
            print(f"{name:<24}{'smoke' if smoke else 'nightly':<9}{why}")
        return 0
    with open(o.budget, encoding="utf-8") as f:
        budget = json.load(f)
    if o.check_budget:
        problems = check.validate_budget(budget, table)
        print("\n".join(problems) if problems else f"budget OK ({len(budget)} cases)")
        return 1 if problems else 0
    names = list(cases_table.SMOKE) if o.smoke else o.cases or list(table)
    unknown = [n for n in names if n not in table]
    if unknown:
        print(f"unknown case(s): {' '.join(unknown)}", file=sys.stderr)
        return 2

    def skip(msg):
        print(("ERROR: " if o.require else "SKIP: ") + msg)
        return 3 if o.require else 0

    if not os.environ.get("TS_SRC_DIR"):
        return skip(
            "TS_SRC_DIR is not set; point it at a built TypeScript v6.0.3 checkout"
        )
    ts_dir = os.path.realpath(os.environ["TS_SRC_DIR"])
    problem = inputs(ts_dir)
    if problem:
        return skip(problem)
    node = shutil.which("node")
    if not node:
        return skip("node not found")
    jsse = os.path.abspath(
        o.jsse
        or os.environ.get("JSSE")
        or os.path.join(ROOT, "target", "release", "jsse")
    )
    if not os.access(jsse, os.X_OK):
        print(f"jsse binary not found or not executable: {jsse}", file=sys.stderr)
        return 1

    oracle_procs.install_cleanup()
    work = os.path.realpath(o.keep_work or tempfile.mkdtemp(prefix="ts-budget-"))
    os.makedirs(work, exist_ok=True)
    known = set(load_known(o.known))
    print(
        f"ts-suite budget: {', '.join(names)} (checkout {ts_dir}, jsse {jsse}, work {work})",
        flush=True,
    )

    def one(name):
        runs = {}
        for engine, exe in (("node", node), ("jsse", jsse)):
            runs[engine] = run_case(
                engine, exe, name, ts_dir, work, o.test_timeout, o.timeout
            )
            r = runs[engine]
            print(
                f"done {name} {engine} {r['seconds']:.0f}s {r['rss_mb']:.0f} MiB exit {r['exit']}",
                flush=True,
            )
        return runs

    try:
        with ThreadPoolExecutor(max(1, min(o.jobs, MAX_JOBS))) as pool:
            results = dict(zip(names, pool.map(one, names)))
    finally:
        if not o.keep_work:
            shutil.rmtree(work, ignore_errors=True)
    problems, rows, report = [], [], {}
    for name in names:
        node_run, jsse_run = results[name]["node"], results[name]["jsse"]
        problems += check.compare(name, node_run, jsse_run, known)
        report[name] = {"node": public(node_run), "jsse": public(jsse_run)}
        if node_run["summary"] and jsse_run["summary"]:
            rows.append((name, len(node_run["summary"][0]), node_run, jsse_run))
            problems += check.check_budget(name, node_run, jsse_run, budget)
    print("\n" + check.table(rows))
    if o.report:
        with open(o.report, "w", encoding="utf-8") as f:
            json.dump(report, f, indent=2)
    if problems:
        print("\nFAIL:\n  " + "\n  ".join(problems))
        return 1
    print("\nOK: outcomes agree and every case is within the budget")
    return 0


if __name__ == "__main__":
    sys.exit(main())
