"""Slices of TypeScript's `conformance` runner for scripts/run-ts-conformance-oracle.py (issue #1276).

The `conformance` runner is the same `CompilerBaselineRunner` as `compiler`, over tests/cases/conformance/**: a suite
is one `describe` named `conformance tests conformance tests for <path>` (one test file, or one configuration of it
when `// @target: es5, es2015` and the like give several), with the six `it`s of the compiler runner (errors, module
resolution tracing, sourcemap record, JS output, sourcemap output, types and symbols).

A slice spec is a '+'-joined union of the terms of scripts/ts_compiler_slices.py (`first:N`, `prefix:A-C`, `shard:I/N`,
`files:a.ts`, `match:REGEX`) plus

    dir:classes,decorators   suites whose test file lies under tests/cases/conformance/<dir> (a path such as
                             classes/classDeclarations works too; ts-suite/oracle.json pins the counts per
                             top-level directory)

or a row of SLICES below. The issue #1276 group is classes, decorators, esDecorators and override.
"""

import re

import ts_compiler_slices as base

GROUP_1276 = ("override", "decorators", "esDecorators", "classes")

SLICES = {d: f"dir:{d}" for d in GROUP_1276}
SLICES["issue1276"] = "dir:" + ",".join(GROUP_1276)

PREFIX = "conformance tests "
SUITE_PREFIX = PREFIX + "conformance tests for "
ROOT = "tests/cases/conformance/"
SUITE_RE = re.compile(
    r"^conformance tests for (" + re.escape(ROOT) + r"\S+?\.\w+)(?: \((.*)\))?$"
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
        self.rel = self.file[len(ROOT) :] if self.file.startswith(ROOT) else self.file
        self.dir = self.rel.split("/", 1)[0] if "/" in self.rel else ""

    def __repr__(self):
        return f"Suite({self.key!r})"


def suites_from_titles(entries):
    """Group `(fullTitle, title)` pairs of the dry run into suites, keeping the runner's order."""
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


def under(rel, dirs):
    return any(rel == d or rel.startswith(d + "/") for d in dirs)


def select(spec, suites, table=None):
    """Suites of `suites` chosen by `spec`, in enumeration order, without duplicates."""
    table = SLICES if table is None else table
    position = {id(s): i for i, s in enumerate(suites)}
    chosen = set()
    for term in base.flat_terms(spec, table):
        kind, _, arg = term.partition(":")
        if kind == "dir" and arg:
            dirs = [d.strip().strip("/") for d in arg.split(",") if d.strip()]
            chosen.update(i for i, s in enumerate(suites) if under(s.rel, dirs))
        else:
            chosen.update(position[id(s)] for s in base.select(term, suites, {}))
    return [suites[i] for i in sorted(chosen)]


def top_dirs(spec, table):
    """The top-level directories a spec of `dir:` terms names, else None (no pin in oracle.json applies)."""
    dirs = []
    for term in base.flat_terms(spec, table):
        kind, _, arg = term.partition(":")
        names = [d.strip().strip("/") for d in arg.split(",") if d.strip()]
        if kind != "dir" or not names or any("/" in n for n in names):
            return None
        dirs += names
    return dirs or None


batches = base.batches
flat_terms = base.flat_terms
char_ranges = base.char_ranges
grep_for = base.grep_for
