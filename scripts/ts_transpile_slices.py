"""Slices of TypeScript's `transpile` runner for scripts/run-ts-transpile-oracle.py (issue #1287).

`TranspileRunner` (src/testRunner/transpileRunner.ts) turns each file of `tests/cases/transpile` into one mocha
`describe` named `transpile tests <path>`. A file yields one test configuration per combination of its
`declarationMap`/`sourceMap`/`inlineSourceMap` settings, and a configuration yields up to two tests: the
`ts.transpileModule` output (`has expected js output`, skipped with `@emitDeclarationOnly`) and the
`ts.transpileDeclaration` output (`has expected declaration output`, only with `@declaration`). Each test compares its
text with `tests/baselines/reference/transpile/<name>.js` or `<name>.d.ts`. One file is one `Test` here; its `tests`
are the full mocha titles.

A slice spec selects files of the runner's enumeration order (the order of the mocha dry run) with the terms of
scripts/ts_compiler_slices.py: `first:N`, `prefix:A-C,x` (first character of the file name, case-insensitive),
`shard:I/N`, `files:a.ts,b.ts`, `match:REGEX`, a '+' union, or the name of a row of SLICES.
"""

import json
import os
import re

import ts_compiler_slices as base

RUNNER = "transpile"
CASES = "tests/cases/transpile"
REFERENCE = "tests/baselines/reference/transpile"

SLICES = {
    "all": "first:100000",  # every file: the 22 files, 37 tests of ts-suite/oracle.json
    "first10": "first:10",
    "declaration": "prefix:D",  # the declaration* files
    "sourcemap": "prefix:J",  # jsWith*SourceMap*
}

TITLE_RE = re.compile(
    r"^transpile tests (tests/cases/transpile/\S+?) (transpile test .*)$"
)


class Test:
    """One test file of the runner; `key` is the mocha full title of its describe."""

    def __init__(self, key, file, tests):
        self.key = key
        self.file = file
        self.tests = tests
        self.base = file.rsplit("/", 1)[-1]

    def __repr__(self):
        return f"Test({self.key!r})"


def tests_from_titles(entries):
    """Group `(fullTitle, title)` pairs of the dry run into files, keeping the runner's order."""
    out = {}
    for full, title in entries:
        if not full.endswith(title):
            continue
        key = full[: len(full) - len(title)].rstrip()
        m = TITLE_RE.match(full)
        if not m or key != f"transpile tests {m.group(1)}":
            continue
        out.setdefault(key, (m.group(1), []))[1].append(full)
    return [Test(k, f, t) for k, (f, t) in out.items()]


select = base.select
flat_terms = base.flat_terms
char_ranges = base.char_ranges


def batches_by_tests(tests, size):
    """Cut `tests` (files) into batches of at most `size` mocha tests (a file is never split)."""
    out, cur, n = [], [], 0
    for t in tests:
        if cur and n + len(t.tests) > size:
            out.append(cur)
            cur, n = [], 0
        cur.append(t)
        n += len(t.tests)
    return out + [cur] if cur else out


def grep_for(batch):
    """A mocha --grep regex selecting exactly the tests of the files of `batch`."""
    names = "|".join(base.js_escape(t.key) for t in batch)
    return f"^(?:{names}) transpile test "


def pinned_expected(spec, table, oracle_json):
    """Passing tests ts-suite/oracle.json pins for a spec that selects the whole runner (the `all` row), else None."""
    if flat_terms(spec, table) != flat_terms(table["all"], table):
        return None
    try:
        with open(oracle_json, encoding="utf-8") as f:
            return json.load(f)["runners"][RUNNER]["passed"]
    except (OSError, ValueError, KeyError):
        return None


def reference_names(test_title):
    """Baseline file names (under REFERENCE) a test title compares against: the js or the declaration output."""
    m = re.match(
        r"^transpile test (.+) has expected (js|declaration) output$", test_title
    )
    if not m:
        return None
    return m.group(1) + (".js" if m.group(2) == "js" else ".d.ts")


def case_path(root, test):
    return os.path.join(root, test.file[len("tests/cases/") :])
