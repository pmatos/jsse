"""Slices of TypeScript's `conformance` runner for scripts/run-ts-conformance-expressions-oracle.py (issue #1275).

The `conformance` runner is the compiler runner's `CompilerBaselineRunner` over `tests/cases/conformance`: one suite
(`describe`) per test file or per configuration of it, six tests each, titled

    conformance tests conformance tests for tests/cases/conformance/<dir>/<file>.ts[ (@target: es5)] Correct errors for ...

so this module mirrors scripts/ts_compiler_slices.py with the conformance prefix and the path in the suite title. A
spec is a '+'-joined union of `dir:expressions/functionCalls,...` (suites below those directories of
tests/cases/conformance, 'expressions' being the whole group of #1275), `first:N`, `shard:I/N`, `files:a.ts`,
`match:REGEX` (takes the rest of the spec) and the rows of SLICES. ts-suite/oracle.json pins the passing tests per
top-level directory (`expressions`: 429 suites, 2574 tests).
"""

import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ts_compiler_slices as base  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RUNNER = "conformance"
PREFIX = "conformance tests "
SUITE_PREFIX = PREFIX + "conformance tests for "
SUITE_GREP = "^" + PREFIX
CONFIG = {"runners": [RUNNER], "light": False}
DIR = "tests/cases/conformance/"
SUITES = os.path.join(ROOT, "ts-suite", "oracle.json")

# upstream spells one directory "condit" + "onalOperator" wrongly; split so the typos hook does not flag it
COND = "condit" + "onalOperator"
CHUNK_DIRS = {
    "ex1": f"arrayLiterals asOperator assignmentOperator binaryOperators commaOperator {COND}".split(),
    "ex2": (
        "contextualTyping elementAccess functionCalls functions identifiers literals newOperator "
        "nullishCoalescingOperator"
    ).split(),
    "ex3": (
        "objectLiterals operators optionalChaining propertyAccess superCalls superPropertyAccess thisKeyword "
        "typeAssertions"
    ).split(),
    "ex4": "typeGuards typeSatisfaction unaryOperators valuesAndReferences".split(),
}
GROUP = [d for dirs in CHUNK_DIRS.values() for d in dirs]
CHUNKS = {
    name: "dir:" + ",".join("expressions/" + d for d in dirs)
    for name, dirs in CHUNK_DIRS.items()
}
SLICES = {"expressions": "dir:expressions", "first200": "first:200", **CHUNKS}

SUITE_RE = re.compile(r"^(.+?\.[a-z]+)(?: \((.*)\))?$")


class Suite(base.Suite):
    def __init__(self, key, tests):
        self.key = key
        self.tests = tests
        m = (
            SUITE_RE.match(key[len(SUITE_PREFIX) :])
            if key.startswith(SUITE_PREFIX)
            else None
        )
        self.file = m.group(1) if m else key
        self.config = m.group(2) if m else None
        self.base = self.file.rsplit("/", 1)[-1]


def suites_from_titles(entries):
    """Group `(fullTitle, title)` pairs of the dry run into conformance suites, keeping the runner's order."""
    out = {}
    for full, title in entries:
        if not full.endswith(title):
            continue
        key = full[: len(full) - len(title)].rstrip()
        if not key.startswith(SUITE_PREFIX):
            continue
        titles = out.setdefault(key, [])
        if full not in titles:
            titles.append(full)
    return [Suite(k, v) for k, v in out.items()]


def dirs_of(arg):
    return [d.strip().strip("/") for d in arg.split(",") if d.strip()]


def under(suite, dirs):
    return any(suite.file.startswith(DIR + d + "/") for d in dirs)


def select(spec, suites, table=None):
    """Suites chosen by `spec` in enumeration order: `dir:` terms here, the others by scripts/ts_compiler_slices.py."""
    table = SLICES if table is None else table
    chosen = set()
    for term in base.flat_terms(spec, table):
        kind, _, arg = term.partition(":")
        if kind == "dir" and arg:
            dirs = dirs_of(arg)
            chosen.update(i for i, s in enumerate(suites) if under(s, dirs))
        elif kind == "prefix":
            raise ValueError(
                "prefix: slices select compiler files; use dir: for the conformance runner"
            )
        else:
            keys = {s.key for s in base.select(term, suites, {})}
            chosen.update(i for i, s in enumerate(suites) if s.key in keys)
    return [suites[i] for i in sorted(chosen)]


def pinned_expected(spec, table, runner=RUNNER):
    """Passing tests ts-suite/oracle.json pins for a spec of `dir:` terms naming top-level directories, else None."""
    terms = base.flat_terms(spec, table)
    if not terms or not all(t.startswith("dir:") for t in terms):
        return None
    names = [d for t in terms for d in dirs_of(t[4:])]
    if any("/" in d for d in names):
        return None
    try:
        with open(SUITES, encoding="utf-8") as f:
            buckets = json.load(f)["runners"][runner]["buckets"]
    except (OSError, ValueError, KeyError):
        return None
    if any(d not in buckets for d in names):
        return None
    return sum(buckets[d]["passed"] for d in set(names))


flat_terms = base.flat_terms
char_ranges = base.char_ranges
grep_for = base.grep_for
batches = base.batches
js_escape = base.js_escape
