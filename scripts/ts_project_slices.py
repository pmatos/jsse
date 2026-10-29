"""Slices of TypeScript's `project` runner for scripts/run-ts-project-oracle.py (issue #1286).

`ProjectRunner` turns each `tests/cases/project/*.json` into two mocha `describe`s, one per module kind
(`@module: commonjs`, `@module: amd`), of four tests each (module resolution tracing, errors, JS output,
declarations): 316 files, 632 suites, the 2528 tests of ts-suite/oracle.json. A suite here is one describe; its
`key` is the mocha full title without the test's own title, and `base` the json file name.

A slice spec selects suites of the runner's enumeration order (the order of the mocha dry run) with the terms of
scripts/ts_compiler_slices.py: `first:N`, `prefix:A-C` (first character of the json file name, case-insensitive),
`shard:I/N`, `files:a.json`, `match:REGEX` (against the suite key), a '+' union, or the name of a row of SLICES.
"""

import re

import ts_compiler_slices as base

# the file-name families of the #1286 matrix: out*, sourcemap*, mapRoot*/maprootUrl*, sourceRoot*/sourcerootUrl*
MATRIX = (
    r"(?i)testcase tests/cases/project/(?:out|sourcemap|maproot|sourceroot)\w*\.json "
)

SLICES = {
    "first20": "first:20",
    "matrix": "match:" + MATRIX,  # #1286: 228 files, 456 suites, 1824 tests
    "out": r"match:(?i)testcase tests/cases/project/out\w*\.json ",
    "sourcemap": r"match:(?i)testcase tests/cases/project/sourcemap\w*\.json ",
    "maproot": r"match:(?i)testcase tests/cases/project/maproot\w*\.json ",
    "sourceroot": r"match:(?i)testcase tests/cases/project/sourceroot\w*\.json ",
    "all": "match:.",
}

PREFIX = "projects tests Compiling project for "
SUITE_RE = re.compile(
    r"^projects tests Compiling project for .*: testcase (tests/cases/project/\S+?\.json) \((@module: \w+)\)$"
)


class Suite:
    """One describe of the project runner (one json file, one module kind)."""

    def __init__(self, key, tests):
        self.key = key
        self.tests = tests
        m = SUITE_RE.match(key)
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
        if not SUITE_RE.match(key):
            continue
        titles = out.setdefault(key, [])
        if full not in titles:
            titles.append(full)
    return [Suite(k, v) for k, v in out.items()]


select = base.select
batches = base.batches
flat_terms = base.flat_terms
char_ranges = base.char_ranges


def grep_for(batch):
    """A mocha --grep regex selecting exactly the tests of the suites of `batch`."""
    return "^(?:" + "|".join(base.js_escape(s.key) for s in batch) + ") Correct "
