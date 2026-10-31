"""Test ids, buckets, selection and the baseline comparison of scripts/run-ts-pass-list.py (issue #1296).

A test is identified exactly as ts-suite/oracle.json identifies it (scripts/run-ts-suite-oracle.py, whose runner and
bucket rules are reused): `parent full title :: test title` with the parent's whitespace normalised. The mocha JSON
reporter gives `fullTitle` = parent full title + " " + title, which is how the id is rebuilt from a dry run.
"""

import collections
import importlib.util
import os
import random
import re

import ts_passlist_scripted as scripted
import ts_passlist_store as store

SCRIPTS = os.path.dirname(os.path.abspath(__file__))
RUNNERS = (
    "compiler",
    "conformance",
    "project",
    "transpile",
    "fourslash",
    "fourslash-server",
    "unittests",
)
ALL_RUNNERS = RUNNERS + tuple(scripted.SCRIPTED)
MOCHA_RUNNER = {"unittests": "unittest"}
PROJECT_FAMILY = re.compile(r"[A-Za-z][a-z]*")
_suite = None


def suite_oracle():
    """scripts/run-ts-suite-oracle.py as a module: the id, runner and bucket rules behind ts-suite/oracle.json."""
    global _suite
    if _suite is None:
        spec = importlib.util.spec_from_file_location(
            "run_ts_suite_oracle", os.path.join(SCRIPTS, "run-ts-suite-oracle.py")
        )
        _suite = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(_suite)
    return _suite


class Test:
    """One mocha test: its id, mocha full title (the grep key), runner, bucket and parent suite."""

    __slots__ = ("id", "full", "parent", "runner", "bucket")

    def __init__(self, full, title):
        self.full = full
        if not full.endswith(title):
            raise ValueError(f"title {title!r} is not the end of {full!r}")
        parent = " ".join(full[: len(full) - len(title)].split())
        self.parent = parent
        self.id = parent + " :: " + title
        self.runner, suite = suite_oracle().classify(parent, title)
        self.bucket = bucket_of(self.runner, suite, parent)

    def __repr__(self):
        return f"Test({self.id!r})"


def bucket_of(runner, suite, parent):
    if runner == "unittests":
        return suite
    if runner == "transpile":
        return "all"
    if runner == "project":
        m = re.search(r"tests/cases/project/(\S+)", parent)
        if not m:
            return "(other)"
        word = PROJECT_FAMILY.match(m.group(1))
        return (word.group(0) if word else m.group(1)[:1]).lower()
    return suite_oracle().bucket(runner, parent) or "(root)"


def tests_from_titles(entries):
    return [Test(full, title) for full, title in entries]


def digest_of(tests):
    return store.sha256_lines(t.id for t in tests)


def suites_of(tests):
    """[(parent, [Test])] in the order of first appearance (a suite is never split)."""
    out = collections.OrderedDict()
    for t in tests:
        out.setdefault(t.parent, []).append(t)
    return list(out.items())


def parse_spec(spec):
    """`runner` or `runner:bucket,bucket` -> (runner, set of buckets or None for all)."""
    runner, _, buckets = spec.partition(":")
    if runner not in ALL_RUNNERS:
        raise ValueError(f"unknown runner {runner!r} (one of {', '.join(ALL_RUNNERS)})")
    return runner, ({b for b in buckets.split(",") if b} if buckets else None)


def select(universe, specs, sample=None, seed=None):
    """The suites to run: {runner: [(parent, [Test])]} for the requested runners/buckets; `sample` keeps that
    fraction of the suites of each runner (at least one), chosen with `seed`."""
    wanted = {}
    for spec in specs or RUNNERS:
        runner, buckets = parse_spec(spec)
        have = wanted.setdefault(runner, set())
        have.update([None] if buckets is None else buckets)
    chosen = {}
    for runner, buckets in wanted.items():
        suites = [
            (p, ts)
            for p, ts in suites_of(universe.get(runner) or [])
            if None in buckets or ts[0].bucket in buckets
        ]
        if sample is not None and suites:
            keep = max(1, round(len(suites) * sample))
            idx = sorted(random.Random(seed).sample(range(len(suites)), keep))
            suites = [suites[i] for i in idx]
        chosen[runner] = suites
    return chosen


def batches(suites, size):
    """Lists of suites holding at most `size` tests each (a bigger suite goes alone)."""
    out, cur, n = [], [], 0
    for item in suites:
        k = len(item[1])
        if cur and n + k > size:
            out.append(cur)
            cur, n = [], 0
        cur.append(item)
        n += k
    if cur:
        out.append(cur)
    return out


def js_escape(text):
    """`text` as a JavaScript regex source matching itself. Mocha cuts a --grep string at the first line
    terminator (`.*`), so those are written as escapes."""
    text = re.sub(r"[.*+?^${}()|\[\]\\/]", lambda m: "\\" + m.group(0), text)
    for raw, esc in (
        ("\n", "\\n"),
        ("\r", "\\r"),
        ("\u2028", "\\u2028"),
        ("\u2029", "\\u2029"),
    ):
        text = text.replace(raw, esc)
    return text


def grep_for(batch):
    """A mocha --grep regex selecting exactly the tests of the suites of `batch`."""
    titles = dict.fromkeys(t.full for _, ts in batch for t in ts)
    return "^(?:" + "|".join(js_escape(t) for t in titles) + ")$"


def outcome_table(outcomes):
    """{full title: (passing, failing, pending)} from the Counter {(title, kind): n} of ts_compiler_report."""
    out = collections.defaultdict(lambda: [0, 0, 0])
    for (title, kind), n in outcomes.items():
        out[title][("passing", "failing", "pending").index(kind)] += n
    return out


def compare(selected, table, in_baseline):
    """Sort the tests of a run. `selected` are Tests, `table` the outcome table of the run, `in_baseline(test)`
    whether the pass list holds the test. A test passes when every one of its occurrences passed; one that failed or
    never reported is a regression when the baseline holds it. Returns {pass, fail, new_pass, regression: [Test],
    held: how many selected tests the pass list holds}."""
    wanted = collections.Counter(t.full for t in selected)
    out = {"pass": [], "fail": [], "new_pass": [], "regression": [], "held": 0}
    for t in selected:
        ok, bad, _ = table.get(t.full, (0, 0, 0))
        passed = bad == 0 and ok >= wanted[t.full]
        held = in_baseline(t)
        out["held"] += bool(held)
        if passed:
            out["pass"].append(t)
            if not held:
                out["new_pass"].append(t)
        elif held:
            out["regression"].append(t)
        else:
            out["fail"].append(t)
    return out
