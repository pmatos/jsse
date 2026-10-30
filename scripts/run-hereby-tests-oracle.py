#!/usr/bin/env python3
"""Build the TypeScript test runner with `hereby tests` on Node and on jsse --node and diff them (issue #1254).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-hereby-tests-oracle.py [options]

TS_SRC_DIR is a TypeScript v6.0.3 checkout with `npm ci` done (it is never written to). For each variant,
`hereby tests` (type-check of src/testRunner by node_modules/typescript, then the esbuild bundle
built/local/run.js) and `hereby tests --no-typecheck` (the bundle only), the script copies the checkout once per
engine, builds on Node and on `jsse --node`, then

  1. compares the build logs (as sorted lines, durations and paths normalised),
  2. compares every file under built/ (run.js and its source map, the tsc -b output of the type-check variant)
     and the generated sources, by relative path, size and sha256 with the copy's path made constant,
  3. lists the tests every runner registers (mocha --dry-run over the bundle, one run per runner: unittest,
     conformance, compiler, project, fourslash, fourslash-server, fourslash-generated, transpile) for the
     Node-built bundle on Node, the jsse-built bundle on Node and on jsse --node, and compares the lists
     line for line (exit status included).

Without TS_SRC_DIR (or with another TypeScript version) it prints SKIP and exits 0 unless --require is given.
Exits 1 on any difference. Options: --jsse PATH (else $JSSE, else target/release/jsse), --work DIR (kept),
--timeout SECONDS per build (default 7200), --list-timeout SECONDS per listing (default 1800),
--variants typecheck,no-typecheck, --runners a,b, --skip-list, --require,
--ci.
"""

import argparse
import hashlib
import importlib.util
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

_spec = importlib.util.spec_from_file_location(
    "run_hereby_local_oracle",
    os.path.join(ROOT, "scripts", "run-hereby-local-oracle.py"),
)
local = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(local)

VARIANTS = {"typecheck": [], "no-typecheck": ["--no-typecheck"]}
RUNNERS = (
    "unittest",
    "conformance",
    "compiler",
    "project",
    "fourslash",
    "fourslash-server",
    "fourslash-generated",
    "transpile",
)
RUN_JS = os.path.join("built", "local", "run.js")


def launch(engine, exe):
    return [exe] + (["--node"] if engine == "jsse" else [])


def hereby_argv(engine, exe, flags):
    return (
        launch(engine, exe)
        + [
            os.path.join("node_modules", "hereby", "bin", "hereby.js"),
            "tests",
        ]
        + flags
    )


def clean_env():
    return {k: v for k, v in os.environ.items() if k not in local.DROP_ENV}


def hereby_tests(engine, exe, tree, flags, timeout):
    """Run `hereby tests` in tree; return (exit code as str, stdout, stderr, seconds)."""
    start = time.monotonic()
    code, out, err = oracle_procs.run_group(
        hereby_argv(engine, exe, flags), tree, clean_env(), timeout
    )
    return (
        code,
        out.decode("utf-8", "replace"),
        err.decode("utf-8", "replace"),
        time.monotonic() - start,
    )


def list_argv(engine, exe):
    return launch(engine, exe) + [
        os.path.join("node_modules", "mocha", "bin", "_mocha"),
        "--dry-run",
        "--reporter=tap",
        "--no-colors",
        "-t",
        "600000",
        RUN_JS,
    ]


def list_tests(engine, exe, tree, runner, timeout):
    """Titles `runner` registers in tree's run.js (mocha --dry-run); return (exit code, stdout, stderr tail)."""
    with open(os.path.join(tree, "test.config"), "w", encoding="utf-8") as f:
        json.dump({"runners": [runner], "light": True, "noColor": True}, f)
    code, out, err = oracle_procs.run_group(
        list_argv(engine, exe), tree, clean_env(), timeout
    )
    return code, out.decode("utf-8", "replace"), err.decode("utf-8", "replace")


def list_summary(text):
    """(number of ok lines, sha256 of the whole listing) for a report line."""
    count = sum(1 for line in text.splitlines() if line.startswith("ok "))
    return count, hashlib.sha256(text.encode()).hexdigest()[:12]


def compare_lists(reference, other, limit=20):
    """Lines describing how two listings differ (empty when identical)."""
    if reference == other:
        return []
    a, b = reference.splitlines(), other.splitlines()
    lines = []
    for i in range(max(len(a), len(b))):
        x = a[i] if i < len(a) else "<missing>"
        y = b[i] if i < len(b) else "<missing>"
        if x != y:
            lines.append(f"line {i + 1}: reference {x!r} / other {y!r}")
            if len(lines) >= limit:
                lines.append("...")
                break
    return lines or ["listings differ only in line endings"]


def build_variant(name, flags, work, ts_dir, engines, timeout):
    """Build one variant on every engine; return (trees, results)."""
    trees, results = {}, {}
    for engine, exe in engines:
        tree = os.path.join(work, name, engine, "ts")
        shutil.rmtree(os.path.dirname(tree), ignore_errors=True)
        os.makedirs(os.path.dirname(tree))
        local.clone_clean(ts_dir, tree)
        trees[engine] = tree
        print(f"hereby tests {' '.join(flags)} [{name}] on {engine} ...", flush=True)
        res = hereby_tests(engine, exe, tree, flags, timeout)
        results[engine] = res
        for stream, text in (("stdout", res[1]), ("stderr", res[2])):
            with open(
                os.path.join(work, name, f"{engine}-{stream}.log"),
                "w",
                encoding="utf-8",
            ) as f:
                f.write(text)
        print(f"  exit {res[0]} in {res[3]:.1f}s", flush=True)
    return trees, results


def compare_builds(name, engines, trees, results):
    """Problems between the Node and the jsse build of one variant."""
    problems = []
    for engine, _ in engines:
        if results[engine][0] != "0":
            problems.append(
                f"[{name}] hereby exited {results[engine][0]} on {engine}:\n"
                + results[engine][2][-3000:]
            )
    logs = {
        engine: local.normalise_log(
            results[engine][1] + results[engine][2], trees[engine], exe
        )
        for engine, exe in engines
    }
    diff = local.diff_logs(logs["node"], logs["jsse"])
    if diff and not problems:
        problems.append(f"[{name}] build logs differ:\n" + "\n".join(diff))
    shots = {engine: local.snapshot(trees[engine]) for engine, _ in engines}
    print(
        f"  [{name}] built files: node {len(shots['node'])}, jsse {len(shots['jsse'])}",
        flush=True,
    )
    for engine in ("node", "jsse"):
        if RUN_JS not in shots[engine]:
            problems.append(f"[{name}] {RUN_JS} was not built on {engine}")
    files = local.compare_snapshots(shots["node"], shots["jsse"])
    if files:
        problems.append(f"[{name}] build output differs:\n" + "\n".join(files[:80]))
    return problems


def compare_listings(name, engines, trees, runners, timeout):
    """List every runner's tests; return (problems, rows for the report)."""
    exes = dict(engines)
    # (bundle built on, engine that lists it); the first is the reference
    combos = [("node", "node"), ("jsse", "node"), ("jsse", "jsse")]
    tasks = [(b, e, r) for r in runners for b, e in combos]

    def one(task):
        built_on, listed_on, runner = task
        return task, list_tests(
            listed_on, exes[listed_on], trees[built_on], runner, timeout
        )

    # test.config is per tree, so listings of one tree must not overlap
    results = {}
    by_tree = {}
    for t in tasks:
        by_tree.setdefault(t[0], []).append(t)

    def run_tree(built_on):
        return [one(t) for t in by_tree[built_on]]

    with ThreadPoolExecutor(max_workers=len(by_tree)) as pool:
        for batch in pool.map(run_tree, list(by_tree)):
            for task, res in batch:
                results[task] = res

    problems, rows = [], []
    for r in runners:
        ref = results[("node", "node", r)]
        count, digest = list_summary(ref[1])
        rows.append((name, r, "node-built/node", ref[0], count, digest))
        if ref[0] != "0" or count == 0:
            problems.append(
                f"[{name}] {r}: reference listing failed (exit {ref[0]}, {count} tests)\n{ref[2][-1500:]}"
            )
            continue
        for built_on, listed_on in combos[1:]:
            got = results[(built_on, listed_on, r)]
            gcount, gdigest = list_summary(got[1])
            rows.append(
                (name, r, f"{built_on}-built/{listed_on}", got[0], gcount, gdigest)
            )
            if got[0] != ref[0]:
                problems.append(
                    f"[{name}] {r}: {built_on}-built bundle on {listed_on} exited {got[0]}, node {ref[0]}\n{got[2][-1500:]}"
                )
            diff = compare_lists(ref[1], got[1])
            if diff:
                problems.append(
                    f"[{name}] {r}: {built_on}-built bundle on {listed_on} lists different tests:\n"
                    + "\n".join(diff)
                )
    return problems, rows


def main():
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("--jsse")
    ap.add_argument("--work")
    ap.add_argument("--timeout", type=int, default=7200)
    ap.add_argument("--list-timeout", type=int, default=1800)
    ap.add_argument("--variants", default=",".join(VARIANTS))
    ap.add_argument("--runners", default=",".join(RUNNERS))
    ap.add_argument("--skip-list", action="store_true")
    ap.add_argument("--require", action="store_true")
    ap.add_argument("--ci", action="store_true")
    o = ap.parse_args()

    def skip(msg):
        if o.require:
            print(f"ERROR: {msg}", file=sys.stderr)
            return 1
        print(f"SKIP: {msg}")
        return 0

    if not os.environ.get("TS_SRC_DIR"):
        return skip(
            "TS_SRC_DIR is not set; point it at a TypeScript v6.0.3 checkout (npm ci done) to run this oracle"
        )
    ts_dir = os.path.realpath(os.environ["TS_SRC_DIR"])
    problem = local.check_checkout(ts_dir)
    if problem:
        return skip(problem)
    variants = [v for v in o.variants.split(",") if v]
    for v in variants:
        if v not in VARIANTS:
            print(f"unknown variant {v}; one of {', '.join(VARIANTS)}", file=sys.stderr)
            return 1
    runners = [r for r in o.runners.split(",") if r]
    for r in runners:
        if r not in RUNNERS:
            print(f"unknown runner {r}; one of {', '.join(RUNNERS)}", file=sys.stderr)
            return 1
    jsse = os.path.abspath(
        o.jsse
        or os.environ.get("JSSE")
        or os.path.join(ROOT, "target", "release", "jsse")
    )
    if not os.access(jsse, os.X_OK):
        print(f"jsse binary not found or not executable: {jsse}", file=sys.stderr)
        return 1
    node = shutil.which("node")
    if not node:
        return skip("node is not on PATH")

    oracle_procs.install_cleanup()
    work = (
        os.path.abspath(o.work)
        if o.work
        else tempfile.mkdtemp(prefix="jsse-hereby-tests-")
    )
    os.makedirs(work, exist_ok=True)
    print(
        f"hereby tests from {ts_dir} (node: {node}, jsse: {jsse}, work: {work})",
        flush=True,
    )
    engines = [("node", node), ("jsse", jsse)]
    problems, rows, times = [], [], []
    for name in variants:
        trees, results = build_variant(
            name, VARIANTS[name], work, ts_dir, engines, o.timeout
        )
        times.append((name, results["node"][3], results["jsse"][3]))
        found = compare_builds(name, engines, trees, results)
        problems += found
        if found or o.skip_list:
            continue
        print(f"  [{name}] listing tests of {', '.join(runners)} ...", flush=True)
        more, table = compare_listings(name, engines, trees, runners, o.list_timeout)
        problems += more
        rows += table
        if not o.work and not found and not more:
            shutil.rmtree(os.path.join(work, name), ignore_errors=True)

    print(f"\n{'variant':<14}{'node s':>10}{'jsse s':>10}")
    for name, a, b in times:
        print(f"{name:<14}{a:>10.1f}{b:>10.1f}")
    if rows:
        print(
            f"\n{'variant':<14}{'runner':<20}{'bundle/engine':<18}{'exit':>5}{'tests':>8}  digest"
        )
        for name, r, how, code, count, digest in rows:
            print(f"{name:<14}{r:<20}{how:<18}{code:>5}{count:>8}  {digest}")
    if problems:
        for p in problems:
            print(
                (
                    "::error title=hereby tests oracle::" + p.replace("\n", "%0A")
                    if o.ci
                    else p
                ),
                file=sys.stderr,
            )
        print(
            f"\nFAIL: hereby tests differs between node and jsse (work dir kept: {work})"
        )
        return 1
    if not o.work:
        shutil.rmtree(work, ignore_errors=True)
    print(
        "\nOK: hereby tests on jsse --node builds the same runner bundle and it lists the same tests under Node and jsse"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
