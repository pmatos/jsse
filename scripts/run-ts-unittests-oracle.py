#!/usr/bin/env python3
"""Run TypeScript's top-level unit tests on Node and on jsse --node and diff them test by test (issue #1259).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-unittests-oracle.py [options] [group-glob ...]

TS_SRC_DIR is a TypeScript v6.0.3 checkout with `npm ci` and `hereby local` done (node_modules/mocha and
built/local/run.js); without it the script prints SKIP and exits 0 unless --require is given. It never writes
into the checkout: each run gets a private working directory of symlinks, like run-ts-mocha-oracle.py, whose
helpers it reuses.

A group is one `src/testRunner/unittests/*.ts` file (scripts/ts_unittests_groups.py). Per group and engine the
script starts mocha 10 on built/local/run.js with the `unittest` runner, the JSON reporter and the group's `-g`
pattern, then compares the sets of test titles that passed and failed:

  * Node must pass every test of the pinned count (the oracle itself is sound);
  * a test that fails on jsse must be listed in ts-suite/unittests-core-known-failures.txt, a test listed
    there must still fail (a fixed bug leaves the list), and a test Node passes and jsse neither passes nor
    fails (it did not run) is a difference;
  * every top-level `describe` of the checkout belongs to exactly one group.

Exit status 1 on any difference. Options: --jobs N (default 4, at most 4; one process per engine per batch of groups), --test-timeout MS (mocha's `-t`,
default 600000: the first Program a suite creates parses lib.d.ts, which takes jsse 25 to 75 s against Node's
0.3 s, so hereby's 40 s default does not fit), --no-light (the Node oracle's light=false, more invariant
checks, about 3x slower on jsse), --timeout SECONDS per group process (default 1800), --jsse PATH (else $JSSE,
else target/release/jsse), --jsse-arg ARG (repeatable, e.g. --no-bytecode), --list, --require, --ci,
--dump DIR (keep the reports and logs).
"""

import argparse
import collections
import fnmatch
import importlib.util
import json
import os
import re
import shutil
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCRIPTS = os.path.join(ROOT, "scripts")
KNOWN_FAILURES = os.path.join(ROOT, "ts-suite", "unittests-core-known-failures.txt")
MAX_JOBS = 4
sys.path.insert(0, SCRIPTS)
import ts_unittests_groups as groups  # noqa: E402
from oracle_procs import install_cleanup, run_group  # noqa: E402

import ts_mocha_cases  # noqa: E402

_cases = len(ts_mocha_cases.CASES)
_spec = importlib.util.spec_from_file_location(
    "run_ts_mocha_oracle", os.path.join(SCRIPTS, "run-ts-mocha-oracle.py")
)
mocha = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(mocha)
del ts_mocha_cases.CASES[
    _cases:
]  # importing the mocha oracle appends its cli and eslint cases to the shared table


def unescape(line):
    """A title line with `\\n` read as a newline and `\\\\` as a backslash (mocha titles may span lines)."""
    return re.sub(r"\\(.)", lambda m: "\n" if m.group(1) == "n" else m.group(1), line)


def read_known_failures(path=KNOWN_FAILURES):
    """Full titles of the tests jsse is known to fail: one per line, `#` comments, see unescape()."""
    try:
        with open(path, encoding="utf-8") as f:
            lines = f.read().splitlines()
    except FileNotFoundError:
        return set()
    return {unescape(ln) for ln in lines if ln and not ln.startswith("#")}


def annotation(line):
    """`line` as a GitHub workflow error command, with the characters that end or split one escaped."""
    escaped = line.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
    return f"::error title=unit-test oracle::{escaped}"


def parse_report(text):
    """(passed titles, failed titles, pending titles) of a mocha JSON report, with multiplicity."""
    doc = json.loads(text)
    pick = lambda key: collections.Counter(t["fullTitle"] for t in doc.get(key, []))  # noqa: E731
    return pick("passes"), pick("failures"), pick("pending")


def compare(group, node, jsse, known):
    """Problems for one group given the (passes, failures, pending) of each engine, `known` failures."""
    problems = []
    n_pass, n_fail, n_pend = node
    j_pass, j_fail, j_pend = jsse
    if n_fail or n_pend:
        problems.append(
            f"Node itself fails or skips {sum(n_fail.values()) + sum(n_pend.values())} tests: "
            + ", ".join(sorted(set(n_fail) | set(n_pend))[:5])
        )
    if sum(n_pass.values()) != group.tests:
        problems.append(
            f"Node passed {sum(n_pass.values())} tests, the group pins {group.tests}"
        )
    ran = j_pass + j_fail + j_pend
    for title in sorted(n_pass - ran):
        problems.append(f"did not run on jsse: {title}")
    for title in sorted(ran - n_pass):
        problems.append(f"ran only on jsse: {title}")
    for title in sorted(set(j_fail) | set(j_pend)):
        if title in n_pass and title not in known:
            problems.append(f"passes on Node, fails on jsse (unlisted): {title}")
    mine = {t for t in known if re.search(group.pattern, t)}
    for title in sorted(mine - set(j_fail) - set(j_pend)):
        problems.append(f"listed as a known failure but no longer fails: {title}")
    return problems


def batches_of(chosen):
    by_batch = {}
    for g in chosen:
        by_batch.setdefault(g.batch, []).append(g)
    return [by_batch[k] for k in sorted(by_batch)]


def split(parsed, group):
    """The part of a (passes, failures, pending) report that belongs to `group`."""
    return tuple(
        collections.Counter(
            {t: n for t, n in part.items() if re.search(group.pattern, t)}
        )
        for part in parsed
    )


def overlaps(parsed, members):
    """Titles selected by more than one group of the batch."""
    titles = set().union(*parsed)
    return sorted(
        t for t in titles if sum(bool(re.search(g.pattern, t)) for g in members) > 1
    )


def run_batch(members, engine, exe, extra, ts_dir, work, o):
    """One mocha process for the groups of a batch: the parsed JSON report, exit code, seconds, stderr tail."""
    name = "batch%d" % members[0].batch
    cfg = {"runners": ["unittest"], "light": not o.no_light}
    case = mocha.cli.Case(name, [], ["RUN"], cfg, {}, "mocha", None, None, ())
    cwd = mocha.prepare(work, engine, case, ts_dir)
    env = {k: v for k, v in os.environ.items() if k not in mocha.DROP_ENV}
    env.update(NODE_ENV="development", TMPDIR=os.path.join(cwd, "tmp"))
    with open(os.path.join(cwd, "test.config"), "w") as f:
        json.dump(cfg, f)
    pattern = "|".join(f"(?:{g.pattern})" for g in members)
    cmd = (
        [exe]
        + extra
        + [
            os.path.join(cwd, "node_modules", "mocha", "bin", "_mocha"),
            "--no-colors",
            "-t",
            str(o.test_timeout),
            "-R",
            "json",
            "-O",
            "output=report.json",
            "-g",
            pattern,
            "built/local/run.js",
        ]
    )
    start = time.monotonic()
    code, out, err = run_group(cmd, cwd, env, o.timeout)
    secs = time.monotonic() - start
    report = os.path.join(cwd, "report.json")
    parsed = None
    if os.path.isfile(report):
        with open(report, encoding="utf-8") as f:
            text = f.read()
        try:
            parsed = parse_report(text)
        except (ValueError, KeyError):
            parsed = None
    if o.dump:
        dest = os.path.join(o.dump, engine, name)
        os.makedirs(dest, exist_ok=True)
        for fname, data in (("stdout", out), ("stderr", err)):
            with open(os.path.join(dest, fname + ".txt"), "wb") as f:
                f.write(data)
        if os.path.isfile(report):
            shutil.copy(report, dest)
    tail = err.decode("utf-8", "replace").strip().splitlines()[-5:]
    return parsed, code, secs, tail


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("groups", nargs="*")
    ap.add_argument("--jobs", type=int, default=MAX_JOBS)
    ap.add_argument("--timeout", type=int, default=1800)
    ap.add_argument("--test-timeout", type=int, default=600000)
    ap.add_argument("--no-light", action="store_true")
    ap.add_argument("--ci", action="store_true")
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--require", action="store_true")
    ap.add_argument("--jsse")
    ap.add_argument("--jsse-arg", action="append", default=[])
    ap.add_argument("--dump")
    o = ap.parse_args()
    o.jobs = max(1, min(o.jobs, MAX_JOBS))

    chosen = groups.GROUPS
    if o.groups:
        unknown = [
            p for p in o.groups if not any(fnmatch.fnmatch(g.name, p) for g in chosen)
        ]
        if unknown:
            sys.exit(f"unknown group: {', '.join(unknown)}")
        chosen = [
            g for g in chosen if any(fnmatch.fnmatch(g.name, p) for p in o.groups)
        ]
    if o.list:
        print("\n".join(g.name for g in chosen))
        return 0

    def skip(msg):
        print(("ERROR: " if o.require else "SKIP: ") + msg)
        return 3 if o.require else 0

    if not os.environ.get("TS_SRC_DIR"):
        return skip(
            "TS_SRC_DIR is not set; point it at a built TypeScript v6.0.3 checkout to run the unit-test oracle"
        )
    ts_dir = os.path.realpath(os.environ["TS_SRC_DIR"])
    problem = mocha.check_checkout(ts_dir)
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
    drift = [] if o.groups else groups.check_groups(ts_dir)
    known = read_known_failures()
    if o.dump:
        o.dump = os.path.abspath(o.dump)
    batches = batches_of(chosen)
    print(
        f"unit tests from {ts_dir} (jsse: {jsse}, {len(chosen)} groups in {len(batches)} processes per engine, "
        f"{o.jobs} jobs, {'full' if o.no_light else 'light'} mode)",
        flush=True,
    )
    install_cleanup()
    work = os.path.realpath(tempfile.mkdtemp(prefix="ts-unittests-oracle-"))

    def one(members):
        ref, rcode, rsecs, rtail = run_batch(members, "node", node, [], ts_dir, work, o)
        got, gcode, gsecs, gtail = run_batch(
            members, "jsse", jsse, ["--node"] + o.jsse_arg, ts_dir, work, o
        )
        rows = []
        for g in members:
            r = split(ref, g) if ref else None
            j = split(got, g) if got else None
            problems = []
            if r is None:
                problems.append(
                    f"Node produced no report (exit {rcode}): {' | '.join(rtail)}"
                )
            if j is None:
                problems.append(
                    f"jsse produced no report (exit {gcode}): {' | '.join(gtail)}"
                )
            if r and j:
                problems += compare(g, r, j, known)
            rows.append((g, r, j, problems))
        extra = [
            f"{e}: test selected by several groups: {t}"
            for e, rep in (("Node", ref), ("jsse", got))
            if rep
            for t in overlaps(rep, members)
        ]
        if not o.ci:
            print(f"done batch of {', '.join(g.name for g in members)}", flush=True)
        return rows, rsecs, gsecs, extra

    try:
        with ThreadPoolExecutor(o.jobs) as pool:
            results = list(pool.map(one, batches))
    finally:
        shutil.rmtree(work, ignore_errors=True)

    count = lambda r: sum(r[0].values()) if r else 0  # noqa: E731
    print(f"\n{'group':<36}{'node':>6}{'jsse':>6}  result")
    bad = 0
    lines = []
    for rows, rs, gs, extra in results:
        bad += len(extra)
        lines += extra
        for g, ref, got, problems in rows:
            bad += bool(problems)
            lines += [f"{g.name}: {p}" for p in problems]
            print(
                f"{g.name:<36}{count(ref):>6}{count(got):>6}  "
                + ("PASS" if not problems else f"FAIL ({len(problems)})")
            )
    lines += [f"groups: {p}" for p in drift]
    for line in lines:
        print(line)
        if o.ci:
            print(annotation(line))
    secs = ", ".join(f"{rs:.0f}s/{gs:.0f}s" for _, rs, gs, _ in results)
    rows = [r for rs in results for r in rs[0]]
    total = {
        "node": sum(count(r[1]) for r in rows),
        "jsse": sum(count(r[2]) for r in rows),
    }
    listed = sum(any(re.search(g.pattern, t) for g in chosen) for t in known)
    print(
        f"\nprocess time node/jsse: {secs}\n"
        f"unit-test oracle: {len(rows) - sum(bool(r[3]) for r in rows)}/{len(rows)} groups match; "
        f"{total['jsse']}/{total['node']} tests present on jsse/Node, {listed} known failures listed"
    )
    return 1 if bad or drift else 0


if __name__ == "__main__":
    sys.exit(main())
