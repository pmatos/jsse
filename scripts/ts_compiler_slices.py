"""Slices of TypeScript's `compiler` runner for scripts/run-ts-compiler-oracle.py (issue #1268).

A suite is one `describe` of `CompilerBaselineRunner`: one test file of tests/cases/compiler, or one
configuration of it (`// @target: es5, es2015` makes one suite per value). Its six `it`s (errors, module
resolution tracing, sourcemap record, JS output, sourcemap output, types and symbols) are the tests.

A slice spec selects suites of the runner's enumeration order (the order of the mocha dry run):

    first:N              the first N suites
    prefix:0-9,A-C,x     suites whose test file name starts with a character of the ranges (case-insensitive;
                         the buckets of ts-suite/oracle.json and of the #1269-#1273 tickets)
    shard:I/N            suites I, I+N, ... of the enumeration, as the runner's own --shards/--shardId pick files
    files:a.ts,b.ts      suites of the named test files (base name or tests/cases/compiler/ path)
    match:REGEX          suites whose title matches the (Python) regular expression
    NAME                 a row of SLICES below (another module's rows come with --slices-module)

A spec is a '+'-joined union (`prefix:A-B+files:x.ts`) of the above; a table row may be such a union.
Add a row to SLICES (or a module of your own passing --slices-module) to name a slice; the oracle needs no change.
"""

import re

SLICES = {
    "first200": "first:200",
    "0-9": "prefix:0-9",
    "a-c": "prefix:0-9,A-C",
}

SUITE_RE = re.compile(r"^compiler tests for (.+?\.tsx?)(?: \((.*)\))?$")
PREFIX = "compiler tests "
IT_TITLES = (
    "Correct errors",
    "Correct module resolution tracing",
    "Correct sourcemap content",
    "Correct JS output",
    "Correct Sourcemap output",
    "Correct type/symbol baselines",
)


class Suite:
    """One describe of the runner: `key` is the mocha full title prefix of its tests."""

    def __init__(self, key, tests):
        self.key = key
        self.tests = tests
        m = SUITE_RE.match(key[len(PREFIX) :]) if key.startswith(PREFIX) else None
        self.file = m.group(1) if m else key
        self.config = m.group(2) if m else None
        self.base = self.file.rsplit("/", 1)[-1]

    def __repr__(self):
        return f"Suite({self.key!r})"


def suites_from_titles(entries):
    """Group `(fullTitle, title)` pairs of the dry run into suites, keeping the runner's order."""
    out = {}
    for full, title in entries:
        if not full.endswith(title):
            continue
        key = full[: len(full) - len(title)].rstrip()
        if not key.startswith(PREFIX + "compiler tests for "):
            continue
        out.setdefault(key, []).append(full)
    return [Suite(k, v) for k, v in out.items()]


def char_ranges(text):
    """'0-9,A-C,x' -> predicate on the first character of a file name (case-insensitive)."""
    spans = []
    for part in text.split(","):
        part = part.strip().lower()
        if re.fullmatch(r"[0-9a-z]-[0-9a-z]", part):
            spans.append((part[0], part[2]))
        elif len(part) == 1:
            spans.append((part, part))
        else:
            raise ValueError(f"bad character range: {part!r}")
    return lambda name: any(lo <= name[:1].lower() <= hi for lo, hi in spans)


def select(spec, suites, table=None, _seen=()):
    """Suites of `suites` chosen by `spec`, in enumeration order, without duplicates."""
    table = SLICES if table is None else table
    chosen = set()
    for term in spec.split("+"):
        term = term.strip()
        kind, _, arg = term.partition(":")
        if kind == "first" and arg.isdigit():
            chosen.update(range(min(int(arg), len(suites))))
        elif kind == "prefix" and arg:
            ok = char_ranges(arg)
            chosen.update(i for i, s in enumerate(suites) if ok(s.base))
        elif kind == "shard" and re.fullmatch(r"\d+/\d+", arg):
            i, n = map(int, arg.split("/"))
            if not 1 <= i <= n:
                raise ValueError(f"shard {arg}: index must be in 1..{n}")
            chosen.update(j for j in range(len(suites)) if j % n == i - 1)
        elif kind == "files" and arg:
            want = {f.strip().rsplit("/", 1)[-1] for f in arg.split(",")}
            chosen.update(i for i, s in enumerate(suites) if s.base in want)
        elif kind == "match" and arg:
            rx = re.compile(arg)
            chosen.update(i for i, s in enumerate(suites) if rx.search(s.key))
        elif term in table and term not in _seen:
            sub = select(table[term], suites, table, _seen + (term,))
            ids = {id(s) for s in sub}
            chosen.update(i for i, s in enumerate(suites) if id(s) in ids)
        else:
            raise ValueError(f"unknown slice: {term!r}")
    return [suites[i] for i in sorted(chosen)]


def js_escape(text):
    return re.sub(r"[.*+?^${}()|\[\]\\/]", lambda m: "\\" + m.group(0), text)


def grep_for(batch):
    """A mocha --grep regex selecting exactly the tests of the suites of `batch`."""
    return "^(?:" + "|".join(js_escape(s.key) for s in batch) + ") Correct "


def batches(suites, size):
    return [suites[i : i + size] for i in range(0, len(suites), size)]
