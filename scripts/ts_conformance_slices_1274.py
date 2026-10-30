"""Slices of TypeScript's `conformance` runner for the `types` directory (issue #1274), for run-ts-compiler-oracle.py.

The `conformance` runner is a second `CompilerBaselineRunner` over tests/cases/conformance: one suite per test file
(or per configuration of it, `// @target: es5, es2015`), titled `conformance tests conformance tests for
tests/cases/conformance/<dir>/<file> [(<configuration>)]`, with the compiler runner's six tests
(`Correct errors for <file>`, module resolution tracing, sourcemap content, JS output, sourcemap output,
type/symbol baselines). This module has the compiler slices' API (`SLICES`, `PREFIX`, `suites_from_titles`, `select`,
`batches`, `grep_for`, `flat_terms`, `char_ranges`) so scripts/run-ts-conformance-types-oracle.py can run the compiler
oracle's machinery on it unchanged.

A spec is a '+' union of the compiler terms (`first:N`, `shard:I/N`, `files:a.ts`, `match:REGEX`, `prefix:A-C` over
the file name) and of

    dir:types/union      suites whose test file is under tests/cases/conformance/types/union/
    NAME                 a row of SLICES below

`types` is the whole directory (857 suites, the 5142 tests of the `types` bucket of ts-suite/oracle.json); the other
rows are its thirty subdirectories, which cover it exactly (checked by scripts/test_ts_conformance_types_1274.py).
"""

import re

import ts_compiler_slices as base

ROOT = "tests/cases/conformance/"
TYPES_DIRS = (
    "any",
    "asyncGenerators",
    "conditional",
    "contextualTypes",
    "forAwait",
    "import",
    "intersection",
    "keyof",
    "literal",
    "localTypes",
    "mapped",
    "members",
    "namedTypes",
    "never",
    "nonPrimitive",
    "objectTypeLiteral",
    "primitives",
    "rest",
    "specifyingTypes",
    "spread",
    "stringLiteral",
    "thisType",
    "tuple",
    "typeAliases",
    "typeParameters",
    "typeRelationships",
    "union",
    "uniqueSymbol",
    "unknown",
    "witness",
)

SLICES = {"types": "dir:types"}
SLICES.update({d: f"dir:types/{d}" for d in TYPES_DIRS})
SLICES["first200"] = SLICES[
    "types"
]  # the oracle's default slice is the whole directory

PREFIX = "conformance tests "
SUITE_RE = re.compile(
    r"^conformance tests for (tests/cases/conformance/\S+?\.\w+)(?: \((.*)\))?$"
)
IT_TITLES = base.IT_TITLES


class Suite(base.Suite):
    """One describe of the conformance runner; `file` is the path of its test file."""

    def __init__(self, key, tests):
        self.key = key
        self.tests = tests
        m = SUITE_RE.match(key[len(PREFIX) :]) if key.startswith(PREFIX) else None
        self.file = m.group(1) if m else key
        self.config = m.group(2) if m else None
        self.base = self.file.rsplit("/", 1)[-1]


def suites_from_titles(entries):
    """Group `(fullTitle, title)` pairs of the dry run into suites, keeping the runner's order."""
    out = {}
    for full, title in entries:
        if not full.endswith(title):
            continue
        key = full[: len(full) - len(title)].rstrip()
        if not key.startswith(PREFIX + "conformance tests for "):
            continue
        titles = out.setdefault(key, [])
        if full not in titles:
            titles.append(full)
    return [Suite(k, v) for k, v in out.items()]


def select(spec, suites, table=None):
    """Suites of `suites` chosen by `spec`, in enumeration order, without duplicates."""
    table = SLICES if table is None else table
    chosen = set()
    for term in base.flat_terms(spec, table):
        kind, _, arg = term.partition(":")
        if kind == "dir" and arg:
            prefix = ROOT + arg.strip("/") + "/"
            chosen.update(i for i, s in enumerate(suites) if s.file.startswith(prefix))
        else:
            wanted = {s.key for s in base.select(term, suites, table)}
            chosen.update(i for i, s in enumerate(suites) if s.key in wanted)
    return [suites[i] for i in sorted(chosen)]


def pinned_directory(spec, table, buckets):
    """Passing tests ts-suite/oracle.json pins for a spec of whole top-level `dir:` terms, else None."""
    terms = base.flat_terms(spec, table)
    names = [t[4:].strip("/") for t in terms if t.startswith("dir:")]
    if not terms or len(names) != len(terms) or any("/" in n for n in names):
        return None
    if any(n not in buckets for n in names):
        return None
    return sum(buckets[n]["passed"] for n in set(names))


batches = base.batches
flat_terms = base.flat_terms
char_ranges = base.char_ranges
js_escape = base.js_escape


def grep_for(batch):
    """A mocha --grep regex selecting exactly the tests of the suites of `batch`."""
    return "^(?:" + "|".join(js_escape(s.key) for s in batch) + ") Correct "
