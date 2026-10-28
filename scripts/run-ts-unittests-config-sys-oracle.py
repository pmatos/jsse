#!/usr/bin/env python3
"""Run TypeScript's config, sys and debugDeprecation unit tests on Node and on jsse --node and diff them per test (issue #1260).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-unittests-config-sys-oracle.py [options] [group ...]

TS_SRC_DIR is a TypeScript v6.0.3 checkout with `npm ci` and `hereby tests` done (node_modules/mocha and
built/local/run.js); without it the script prints SKIP and exits 0 unless --require is given. For every group of
scripts/ts_unittest_groups.py it starts mocha's CLI (the way `hereby runtests` does) on the `unittest` runner
(`-g <group regex>`, light mode off, as `hereby runtests-parallel --light=false` runs it) once per engine with the
json reporter, in a private working directory of symlinks into the checkout (the checkout is never written to), and
compares the title of every test with its outcome (passing, failing, pending) between the engines.

A test that passes on Node and fails on jsse must be listed in ts-suite/unittests-config-sys.json under
"known_failures" with the issue that tracks it (ts-suite/README.md); a listed test that now passes, a failure that
is not listed, a Node failure and a changed test count (the "counts" of the file) all exit 1. Titles repeat in a few
suites, so outcomes are compared as multisets. Never builds anything.

Options: --jobs N (default 4, at most 4), --timeout SECONDS (default 1800), --list, --require, --verbose,
--jsse PATH (else $JSSE, else target/release/jsse), --baseline FILE, --record (rewrite the counts and keep the
known failures that still fail), --keep-work DIR (leave the per-group reports there).
"""

import argparse
import collections
import fnmatch
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ts_unittest_groups import GROUPS  # noqa: E402

BASELINE = os.path.join(ROOT, "ts-suite", "unittests-config-sys.json")
SUITES = os.path.join(ROOT, "ts-suite", "oracle.json")
SUITE_OF = {"config": "config-", "sys": "sys-", "debugDeprecation": "debugDeprecation"}
UNPINNED_GROUPS = ()  # groups whose tests the pin does not count under their suite
DROP_ENV = (
    "NODE_OPTIONS",
    "FORCE_COLOR",
    "NO_COLOR",
    "COLUMNS",
    "NODE_DISABLE_COLORS",
    "CI",
)
LINKS = ("built", "node_modules", "scripts", "lib", "src", "package.json")
TEST_INPUTS = ("cases", "lib")
MAX_JOBS = 4
CONFIG = {"runners": ["unittest"], "light": False}
TEST_TIMEOUT_MS = 40000
DEFAULT_GROUPS = None


def prepare(work, engine, group):
    cwd = os.path.join(work, engine, group)
    os.makedirs(os.path.join(cwd, "tests", "baselines", "local"))
    for link in LINKS:
        os.symlink(os.path.join(TS_DIR, link), os.path.join(cwd, link))
    for link in TEST_INPUTS:
        os.symlink(
            os.path.join(TS_DIR, "tests", link), os.path.join(cwd, "tests", link)
        )
    os.symlink(
        os.path.join(TS_DIR, "tests", "baselines", "reference"),
        os.path.join(cwd, "tests", "baselines", "reference"),
    )
    os.makedirs(os.path.join(cwd, "tmp"))
    with open(os.path.join(cwd, "test.config"), "w") as f:
        json.dump(CONFIG, f)
    return cwd


def launch(cmd, cwd, env, timeout):
    start = time.monotonic()
    child = subprocess.Popen(
        cmd,
        cwd=cwd,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        start_new_session=True,
    )
    try:
        out, err = child.communicate(timeout=timeout)
        code = str(child.returncode)
    except subprocess.TimeoutExpired:
        os.killpg(child.pid, signal.SIGKILL)
        out, err = child.communicate()
        code = "timeout"
    else:
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    return code, out, err, time.monotonic() - start


def run_group(engine, exe, group, regex, work, timeout):
    """One mocha run; returns {'outcomes': Counter of (title, outcome), 'errors': {title: first line}, ...}."""
    cwd = prepare(work, engine, group)
    env = {k: v for k, v in os.environ.items() if k not in DROP_ENV}
    env["NODE_ENV"] = "development"
    env["TMPDIR"] = os.path.join(cwd, "tmp")
    mocha = os.path.join(cwd, "node_modules", "mocha", "bin", "_mocha")
    report = os.path.join(cwd, "report.json")
    cmd = [exe] + (["--node"] if engine == "jsse" else [])
    cmd += [mocha, "--no-colors", "-t", str(TEST_TIMEOUT_MS)]
    cmd += ["-R", "json", "-O", f"output={report}"]
    cmd += ["-g", regex, "built/local/run.js"]
    code, out, err, secs = launch(cmd, cwd, env, timeout)
    res = {
        "exit": code,
        "secs": secs,
        "outcomes": collections.Counter(),
        "errors": {},
        "tail": (out + err).decode("utf-8", "replace")[-1500:],
        "report": None,
    }
    try:
        with open(report, encoding="utf-8") as f:
            doc = json.load(f)
    except (OSError, ValueError):
        return res
    res["report"] = doc
    for kind, key in (
        ("passing", "passes"),
        ("failing", "failures"),
        ("pending", "pending"),
    ):
        for t in doc.get(key, []):
            title = t.get("fullTitle", "")
            res["outcomes"][(title, kind)] += 1
            if kind == "failing":
                msg = (t.get("err") or {}).get("message") or ""
                res["errors"].setdefault(
                    title, msg.strip().splitlines()[0][:300] if msg.strip() else ""
                )
    return res


def counts(res):
    c = collections.Counter()
    for (_, kind), n in res["outcomes"].items():
        c[kind] += n
    return {k: c.get(k, 0) for k in ("passing", "failing", "pending")}


Verdict = collections.namedtuple(
    "Verdict", "regress unexpected stale node_fail differing"
)


def classify(node, jsse, known):
    """Titles of Node passes that fail on jsse, split into known and unexpected, stale baseline entries, and
    titles whose passing/pending outcomes (with multiplicity) differ between the engines without a failure."""
    node_pass = collections.Counter()
    for (title, kind), n in node["outcomes"].items():
        if kind == "passing":
            node_pass[title] += n
    jsse_fail = {t for (t, kind) in jsse["outcomes"] if kind == "failing"}
    node_fail = sorted({t for (t, kind) in node["outcomes"] if kind == "failing"})
    regress = sorted(t for t in jsse_fail if node_pass[t])
    only_jsse = sorted(jsse_fail - set(regress) - set(node_fail))
    unexpected = [t for t in regress + only_jsse if t not in known]
    stale = sorted(t for t in known if t not in jsse_fail)
    ok = {k: v for k, v in node["outcomes"].items() if k[1] != "failing"}
    got = {k: v for k, v in jsse["outcomes"].items() if k[1] != "failing"}
    differing = sorted(
        {
            t
            for (t, kind) in set(ok) | set(got)
            if ok.get((t, kind), 0) != got.get((t, kind), 0)
            and t not in jsse_fail
            and t not in node_fail
        }
    )
    return Verdict(regress, unexpected, stale, node_fail, differing)


def check_checkout(ts_dir):
    mocha = os.path.join(ts_dir, "node_modules", "mocha", "bin", "_mocha")
    run_js = os.path.join(ts_dir, "built", "local", "run.js")
    manifest = os.path.join(ts_dir, "package.json")
    if not os.path.isfile(mocha) or not os.path.isfile(manifest):
        return f"{ts_dir} has no node_modules/mocha (run npm ci in the TypeScript checkout)"
    if not os.path.isfile(run_js):
        return f"{ts_dir} has no built/local/run.js (run hereby tests in the TypeScript checkout)"
    try:
        with open(manifest, encoding="utf-8") as f:
            version = json.load(f).get("version", "unknown")
    except (OSError, ValueError, AttributeError):
        version = "unknown"
    if version != "6.0.3":
        return f"the unit-test oracle targets TypeScript 6.0.3, found {version}"
    return None


def pinned_suite_problems(new_counts):
    """The per-suite passing counts of ts-suite/oracle.json (recorded on Node by the pin) equal this run's."""
    try:
        with open(SUITES, encoding="utf-8") as f:
            suites = json.load(f)["runners"]["unittests"]["suites"]
    except (OSError, ValueError, KeyError):
        return []
    problems = []
    for suite, prefix in SUITE_OF.items():
        got = sum(
            c["passing"]
            for n, c in new_counts.items()
            if n.startswith(prefix) and n not in UNPINNED_GROUPS
        )
        want = suites.get(suite, {}).get("passed")
        if want != got:
            problems.append(
                f"suite {suite}: oracle.json pins {want} passing tests, Node ran {got}"
            )
    return problems


def load_baseline(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def main():
    global TS_DIR
    ap = argparse.ArgumentParser()
    ap.add_argument("groups", nargs="*")
    ap.add_argument("--jobs", type=int, default=MAX_JOBS)
    ap.add_argument("--timeout", type=int, default=1800)
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--require", action="store_true")
    ap.add_argument("--verbose", action="store_true")
    ap.add_argument("--record", action="store_true")
    ap.add_argument("--jsse")
    ap.add_argument("--baseline", default=BASELINE)
    ap.add_argument("--keep-work")
    o = ap.parse_args()

    names = list(GROUPS if o.groups else DEFAULT_GROUPS or GROUPS)
    if o.groups:
        unknown = [p for p in o.groups if not any(fnmatch.fnmatch(n, p) for n in names)]
        if unknown:
            sys.exit(f"unknown group: {', '.join(unknown)}")
        names = [n for n in names if any(fnmatch.fnmatch(n, p) for p in o.groups)]
    if o.list:
        print("\n".join(names))
        return 0

    def skip(msg):
        print(("ERROR: " if o.require else "SKIP: ") + msg)
        return 3 if o.require else 0

    if not os.environ.get("TS_SRC_DIR"):
        return skip(
            "TS_SRC_DIR is not set; point it at a built TypeScript v6.0.3 checkout to run the unit-test oracle"
        )
    TS_DIR = os.path.realpath(os.environ["TS_SRC_DIR"])
    problem = check_checkout(TS_DIR)
    if problem:
        return skip(problem)
    node = shutil.which("node")
    if not node:
        return skip("node not found; the oracle needs it")
    given = o.jsse or os.environ.get("JSSE")
    jsse = os.path.abspath(given or os.path.join(ROOT, "target", "release", "jsse"))
    if not os.access(jsse, os.X_OK):
        print(f"jsse binary not found or not executable: {jsse}", file=sys.stderr)
        return 1

    base = load_baseline(o.baseline) if os.path.exists(o.baseline) else {}
    known = base.get("known_failures", {})
    jobs = max(1, min(o.jobs, MAX_JOBS))
    print(
        f"unit tests from {TS_DIR} (jsse: {jsse}, {len(names)} groups, {jobs} jobs)",
        flush=True,
    )
    work = os.path.realpath(tempfile.mkdtemp(prefix="ts-unittests-oracle-"))

    def one(name):
        ref = run_group("node", node, name, GROUPS[name], work, o.timeout)
        got = run_group("jsse", jsse, name, GROUPS[name], work, o.timeout)
        print(f"done {name}", flush=True)
        return name, ref, got

    try:
        with ThreadPoolExecutor(jobs) as pool:
            results = list(pool.map(one, names))
        failed = False
        new_counts = {}
        titles = []
        print(f"\n{'group':<40}{'node':>22}{'jsse':>22}{'node s':>8}{'jsse s':>8}")
        for name, ref, got in results:
            rc, gc = counts(ref), counts(got)
            new_counts[name] = rc
            fmt = lambda c: f"{c['passing']}/{c['failing']}/{c['pending']}"  # noqa: E731
            print(
                f"{name:<40}{fmt(rc):>22}{fmt(gc):>22}{ref['secs']:>8.1f}{got['secs']:>8.1f}"
            )
            titles += [t for (t, _) in ref["outcomes"]]
        print("(columns are passing/failing/pending)")
        total_node = sum(c["passing"] for c in new_counts.values())
        total_jsse = 0
        for name, ref, got in results:
            lines = []
            if ref["report"] is None or got["report"] is None:
                lines.append(
                    "no report written: node exit "
                    + ref["exit"]
                    + ", jsse exit "
                    + got["exit"]
                )
                for eng, r in (("node", ref), ("jsse", got)):
                    if r["report"] is None:
                        lines.append(f"  {eng} output tail:\n" + r["tail"])
            mine = {t for t in known if re.search(GROUPS[name], t)}
            regress, unexpected, stale, node_fail, differing = classify(ref, got, mine)
            total_jsse += counts(got)["passing"]
            for t in node_fail:
                lines.append(f"fails on Node itself: {t}")
            for t in unexpected:
                lines.append(
                    f"passes on Node, fails on jsse (not in the baseline): {t}\n    {got['errors'].get(t, '')}"
                )
            for t in differing:
                lines.append(
                    f"ran a different number of times (or not at all) on jsse: {t}"
                )
            for t in stale:
                lines.append(f"baseline lists a test that no longer fails on jsse: {t}")
            if o.verbose:
                for t in regress:
                    if t in known:
                        lines.append(f"known failure: {t} ({known[t]})")
            expected = base.get("counts", {}).get(name)
            if expected is not None and expected != counts(ref):
                lines.append(
                    f"test count changed: baseline {expected}, Node now {counts(ref)}"
                )
            if lines:
                failed = True
                print(f"\n{name}:")
                for line in lines:
                    print("  " + line)
        if not o.groups:
            overlap = [
                t
                for t in set(titles)
                if sum(bool(re.search(GROUPS[n], t)) for n in GROUPS) > 1
            ]
            for t in overlap:
                failed = True
                print(f"\ntitle matched by several groups: {t}")
            for problem in pinned_suite_problems(new_counts):
                failed = True
                print(f"\n{problem}")
        print(f"\nnode passing {total_node}, jsse passing {total_jsse}")
        if o.record:
            return record(o.baseline, base, new_counts, results)
        print("unit-test oracle: " + ("FAIL" if failed else "PASS"))
        return 1 if failed else 0
    finally:
        if o.keep_work:
            shutil.copytree(work, o.keep_work, dirs_exist_ok=True)
        shutil.rmtree(work, ignore_errors=True)


def record(path, base, new_counts, results):
    """Rewrite the baseline for the groups just run; the other groups' entries stay as they are."""
    bad = [
        f"{name}: no report from {eng}"
        for name, ref, got in results
        for eng, r in (("node", ref), ("jsse", got))
        if r["report"] is None
    ] + [
        f"{name}: {t} fails on Node"
        for name, ref, got in results
        for t in classify(ref, got, set()).node_fail
    ]
    if bad:
        print("refusing to record:\n  " + "\n  ".join(bad))
        return 1
    selected = [name for name, _, _ in results]
    old = base.get("known_failures", {})
    known = {
        t: why
        for t, why in old.items()
        if not any(re.search(GROUPS[n], t) for n in selected)
    }
    for _, ref, got in results:
        v = classify(ref, got, set())
        for t in v.regress + v.unexpected:
            known[t] = old.get(t, "TODO: file the bug and put its number here")
    merged = {**base.get("counts", {}), **new_counts}
    counts_all = {n: merged[n] for n in GROUPS if n in merged}
    with open(path, "w", encoding="utf-8") as f:
        f.write('{\n "counts": {\n')
        f.write(
            ",\n".join(
                f"  {json.dumps(n)}: {json.dumps(c)}" for n, c in counts_all.items()
            )
        )
        f.write('\n },\n "known_failures": ')
        f.write(
            json.dumps(
                dict(sorted(known.items())), indent=1, ensure_ascii=False
            ).replace("\n", "\n ")
        )
        f.write("\n}\n")
    print(f"wrote {path}: {len(known)} known failures")
    return 0


TS_DIR = ""

if __name__ == "__main__":
    sys.exit(main())
