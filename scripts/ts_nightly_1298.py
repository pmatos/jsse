"""Sharding, result files and the merge verdict of the nightly TypeScript suite job (issue #1298).

scripts/run-ts-suite-nightly-1298.py drives scripts/run-ts-pass-list.py (#1296) over one shard of the suites and
writes a results directory per shard; its `merge` command folds the shard directories into one report. This module
is the logic of both, standard library only, so the unit tests need no TypeScript checkout.
"""

import collections
import heapq
import json
import os

SCHEMA = 1
# Seconds of one job per test (wall seconds of a mocha batch at --jobs 2) measured by the first full run of this
# workflow on GitHub-hosted runners; only the ratios matter, they balance the shards. Refresh them from the
# timings.json of a nightly run. tsc-watch is per case (a session settles for seconds).
WEIGHT = {
    "compiler": 0.71,
    "conformance": 0.48,
    "unittests": 1.12,
    "fourslash": 0.46,
    "fourslash-server": 2.75,
    "project": 0.22,
    "transpile": 1.7,
    "tsc-projects": 1.9,
    "tsc-watch": 11.0,
    "hereby": 3.0,
}
# The runners of the nightly job. tsc-projects is left out: its recorded bucket in ts-suite/jsse-pass-1296.json
# holds 367 cases, scripts/tsc-fixtures has 413 since #1488, and a recorded bucket may not change (the ratchet), so
# the gate reports the baseline as being for another test set. Add it back once the bucket is re-recorded.
DEFAULT_RUNNERS = (
    "compiler",
    "conformance",
    "project",
    "transpile",
    "fourslash",
    "fourslash-server",
    "unittests",
    "tsc-watch",
    "hereby",
)


def line_of(test_id):
    """A test id as one line of a pass list file: the escaping of node-pass-list.txt (backslash, LF, CR)."""
    return test_id.replace("\\", "\\\\").replace("\n", "\\n").replace("\r", "\\r")


def partition(chosen, shard, shards):
    """The suites of `chosen` ({runner: [(parent, [test])]}) that shard `shard` of `shards` runs.

    Longest-processing-time-first over the estimated cost of each suite (WEIGHT per test), ties broken by name, so
    every shard computes the same assignment from the same universe and each suite lands in exactly one shard."""
    if not 0 <= shard < shards:
        raise ValueError(f"shard {shard} is not in 0..{shards - 1}")
    items = sorted(
        (
            (-WEIGHT.get(runner, 1.0) * len(ts), runner, parent)
            for runner, suites in chosen.items()
            for parent, ts in suites
        )
    )
    loads = [(0.0, i) for i in range(shards)]
    heapq.heapify(loads)
    mine = set()
    for cost, runner, parent in items:
        load, i = heapq.heappop(loads)
        heapq.heappush(loads, (load - cost, i))
        if i == shard:
            mine.add((runner, parent))
    # A runner without a suite here is dropped: the gate would run a scripted oracle with no case names, i.e. all of them.
    mine_by_runner = {
        runner: [(p, ts) for p, ts in suites if (runner, p) in mine]
        for runner, suites in chosen.items()
    }
    return {runner: suites for runner, suites in mine_by_runner.items() if suites}


LIST_PREFIXES = ("pass", "fail", "regression")


def write_json(path, doc, **kw):
    with open(path, "w", encoding="utf-8") as f:
        json.dump(doc, f, indent=1, **kw)


def write_shard(out_dir, summary, passed, failed, regressed, timings):
    """Write one shard's results: summary.json, timings.json and the id lists per runner."""
    os.makedirs(out_dir, exist_ok=True)
    for prefix, by_runner in zip(LIST_PREFIXES, (passed, failed, regressed)):
        for runner, ids in by_runner.items():
            if ids:
                with open(
                    os.path.join(out_dir, f"{prefix}-{runner}.txt"),
                    "w",
                    encoding="utf-8",
                ) as f:
                    f.write("".join(line_of(i) + "\n" for i in sorted(ids)))
    write_json(os.path.join(out_dir, "timings.json"), timings)
    write_json(os.path.join(out_dir, "summary.json"), summary, sort_keys=True)


def find_shards(root):
    """{directory: summary} of every summary.json under `root`."""
    found = {}
    for dirpath, _, files in sorted(os.walk(root)):
        if "summary.json" in files:
            try:
                with open(os.path.join(dirpath, "summary.json"), encoding="utf-8") as f:
                    found[dirpath] = json.load(f)
            except (OSError, ValueError):
                found[dirpath] = None
    return found


def check_shards(found, shards, runners, sampled):
    """Integrity problems of the set of shard summaries: a shard missing, duplicated or that never ran, a universe
    that differs between shards or from the sum of what they ran (a suite run twice or by nobody)."""
    problems = []
    by_shard = collections.defaultdict(list)
    for path, s in found.items():
        if not isinstance(s, dict) or s.get("schema") != SCHEMA:
            problems.append(f"{path}: summary.json is not a schema {SCHEMA} summary")
            continue
        if s.get("shards") != shards:
            problems.append(
                f"{path}: written for {s.get('shards')} shards, expected {shards}"
            )
        by_shard[s.get("shard")].append(s)
    for i in range(shards):
        got = by_shard.get(i, [])
        if not got:
            problems.append(
                f"shard {i}: no results (job failed before writing them, or was cancelled)"
            )
        elif len(got) > 1:
            problems.append(f"shard {i}: {len(got)} result directories")
    ran = []
    reported = collections.defaultdict(list)
    for got in by_shard.values():
        for s in got:
            if s.get("ran"):
                ran.append(s)
            else:
                problems.append(
                    f"shard {s.get('shard')}: the pass list gate did not run (exit {s.get('exit')})"
                )
            for p in s.get("integrity", []):
                reported[p].append(s.get("shard"))
    for text, who in reported.items():
        shown = f"shard {who[0]}" if len(who) == 1 else f"{len(who)} shards"
        problems.append(f"{shown}: {text}")
    if not ran:
        return problems
    universes = {json.dumps(s["universe"], sort_keys=True) for s in ran}
    if len(universes) > 1:
        problems.append("the shards listed different test universes")
    universe = ran[0]["universe"]
    totals = collections.Counter()
    for s in ran:
        totals.update(s["selected"])
    for runner in runners:
        if runner not in universe:
            problems.append(f"{runner}: not in the universe (runner unavailable?)")
        elif not sampled and totals[runner] != universe[runner]:
            problems.append(
                f"{runner}: shards ran {totals[runner]} tests, the universe has {universe[runner]}"
            )
    return problems


def merge(root, out_dir, shards, runners, blocking=False, sampled=False):
    """Fold the shard directories under `root` into `out_dir`: merged pass lists, timings.json, summary.json and
    summary.md. Returns (exit status, [message lines]); messages starting with `error:` or `warning:` are
    annotations."""
    found = find_shards(root)
    problems = check_shards(found, shards, runners, sampled)
    shard_dirs = {
        p: s for p, s in found.items() if isinstance(s, dict) and s.get("ran")
    }
    tables = collections.defaultdict(collections.Counter)
    lists = collections.defaultdict(list)
    timings, batch_errors = [], []
    for path, s in sorted(shard_dirs.items(), key=lambda kv: kv[1].get("shard", -1)):
        for runner, row in s["runners"].items():
            tables[runner].update(row)
        batch_errors += [f"shard {s['shard']}: {e}" for e in s.get("batch_errors", [])]
        try:
            with open(os.path.join(path, "timings.json"), encoding="utf-8") as f:
                timings += [{"shard": s["shard"], **t} for t in json.load(f)]
        except (OSError, ValueError):
            problems.append(f"shard {s['shard']}: timings.json missing or unreadable")
        for name in sorted(os.listdir(path)):
            stem, ext = os.path.splitext(name)
            if ext == ".txt" and stem.split("-", 1)[0] in LIST_PREFIXES:
                with open(os.path.join(path, name), encoding="utf-8") as f:
                    lists[stem].extend(f.read().split("\n")[:-1])
    os.makedirs(out_dir, exist_ok=True)
    for stem, lines in lists.items():
        with open(os.path.join(out_dir, stem + ".txt"), "w", encoding="utf-8") as f:
            f.write("".join(line + "\n" for line in sorted(lines)))
    write_json(os.path.join(out_dir, "timings.json"), timings)
    total = collections.Counter()
    for row in tables.values():
        total.update(row)
    gating = []
    if total["regression"]:
        gating.append(f"{total['regression']} baseline regression(s)")
    gating += batch_errors
    summary = {
        "schema": SCHEMA,
        "shards": shards,
        "blocking": blocking,
        "runners": {r: dict(c) for r, c in sorted(tables.items())},
        "total": dict(total),
        "integrity_problems": problems,
        "gating_problems": gating,
    }
    write_json(os.path.join(out_dir, "summary.json"), summary, sort_keys=True)
    with open(os.path.join(out_dir, "summary.md"), "w", encoding="utf-8") as f:
        f.write(markdown(summary, timings, shard_dirs))
    messages = [f"error: {p}" for p in problems]
    level = "error" if blocking else "warning"
    messages += [f"{level}: {g}" for g in gating]
    failed = bool(problems) or (blocking and bool(gating))
    return (1 if failed else 0), messages


def markdown(summary, timings, shard_dirs):
    rows = [
        "| runner | baseline | pass | fail | new-pass | regression |",
        "|---|--:|--:|--:|--:|--:|",
    ]

    def row(name, c):
        cells = (
            c.get(k, 0) for k in ("held", "pass", "fail", "new_pass", "regression")
        )
        return f"| {name} | " + " | ".join(map(str, cells)) + " |"

    rows += [row(r, c) for r, c in summary["runners"].items()]
    rows.append(row("**total**", summary["total"]))
    secs = collections.Counter()
    for x in timings:
        if not x.get("dry"):
            secs[x["runner"]] += x["secs"]
    wall = {s["shard"]: s.get("secs", 0) for s in shard_dirs.values()}
    out = [
        f"### TypeScript suite on jsse ({'blocking' if summary['blocking'] else 'non-blocking'})",
        "",
        *rows,
        "",
        "Job time per runner (sum over batches, seconds): "
        + ", ".join(f"{r} {v:.0f}" for r, v in sorted(secs.items())),
        "",
        "Shard wall time (seconds): "
        + ", ".join(f"{k}: {v:.0f}" for k, v in sorted(wall.items())),
        "",
    ]
    for title, key in (
        ("Integrity problems", "integrity_problems"),
        ("Regressions and crashed batches", "gating_problems"),
    ):
        if summary[key]:
            out += (
                [f"**{title}**", ""]
                + [f"- {p[:300]}" for p in summary[key][:30]]
                + [""]
            )
    return "\n".join(out)


def timing_record(runner, tag, tests, secs, exit_code, dry=False):
    return {
        "runner": runner,
        "tag": tag,
        "tests": tests,
        "secs": round(secs, 2),
        "exit": exit_code,
        "dry": dry,
    }
