"""Slices of TypeScript's `fourslash` runners for scripts/run-ts-fourslash-oracle.py (issue #1288).

`FourSlashRunner` turns each `tests/cases/fourslash/*.ts` (the `fourslash` runner) or each
`tests/cases/fourslash/server/*.ts` (the `fourslash-server` runner) into one mocha `describe` named by the file path
whose test is `<runner> test <file> runs correctly` (the server runner adds `<runner> test <file> tsserver log`).
One file is one `Test` here; its `tests` are the full mocha titles.

A slice spec selects tests of the runner's enumeration order (the order of the mocha dry run, i.e. sorted file names)
with the terms of scripts/ts_compiler_slices.py: `first:N`, `prefix:A-C,x` (first character of the file name,
case-insensitive; the buckets of ts-suite/oracle.json), `shard:I/N`, `files:a.ts,b.ts`, `match:REGEX`, a '+' union,
or the name of a row of SLICES below.

The issues that follow #1288 only add rows to SLICES (or pass --slices-module NAME for a module of their own with a
`SLICES` dict); `RUNNERS` says which runner a bare run uses. Rows are shared by both runners, so a server row is a
name range of the `server` directory.
"""

import re

import ts_compiler_slices as base

# runner name -> (path of its test directory, tests per file)
RUNNERS = {
    "fourslash": ("tests/cases/fourslash", 1),
    "fourslash-server": ("tests/cases/fourslash/server", 2),
}

# issue -> row: the ranges of the #1288-#1293 split (file name first character, case-insensitive)
SLICES = {
    "first200": "first:200",  # #1288
    "a-c": "prefix:0-9,A-C",  # #1289
    "d-g": "prefix:D-G",  # #1290
    "h-m": "prefix:H-M",  # #1291
    "n-r": "prefix:N-R",  # #1292
    "s-z": "prefix:S-Z",  # #1293
    "all": "prefix:0-9,A-Z",
}

TITLE_RE = re.compile(r"^(fourslash(?:-server)?) tests (tests/cases/\S+?\.tsx?) (.*)$")


class Test:
    """One test file of a fourslash runner; `key` is the mocha full title of its describe."""

    def __init__(self, key, file, tests):
        self.key = key
        self.file = file
        self.tests = tests
        self.base = file.rsplit("/", 1)[-1]

    def __repr__(self):
        return f"Test({self.key!r})"


def tests_from_titles(entries, runner):
    """Group `(fullTitle, title)` pairs of the dry run into files, keeping the runner's order."""
    out = {}
    for full, title in entries:
        if not full.endswith(title):
            continue
        key = full[: len(full) - len(title)].rstrip()
        m = TITLE_RE.match(full)
        if not m or m.group(1) != runner or key != f"{runner} tests {m.group(2)}":
            continue
        out.setdefault(key, (m.group(2), []))[1].append(full)
    return [Test(k, f, t) for k, (f, t) in out.items()]


select = base.select
batches = base.batches
flat_terms = base.flat_terms
char_ranges = base.char_ranges


def grep_for(batch, runner):
    """A mocha --grep regex selecting exactly the tests of the files of `batch`."""
    names = "|".join(base.js_escape(t.key) for t in batch)
    return f"^(?:{names}) {base.js_escape(runner)} test "
