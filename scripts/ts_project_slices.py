"""Slices of TypeScript's `project` runner for scripts/run-ts-project-oracle.py (issue #1285).

`ProjectRunner` turns each `tests/cases/project/**/*.json` (316 files on v6.0.3) into two mocha `describe`s, one per
module kind (`@module: commonjs`, `@module: amd`), of four tests each (module resolution tracing, errors, JS output,
declarations): 2528 tests. A `Case` here is one JSON file; its `tests` are the full mocha titles of both describes.

A slice spec selects cases in the runner's enumeration order (the order of the mocha dry run, i.e. sorted file
names) with the terms of scripts/ts_compiler_slices.py (`first:N`, `prefix:A-C`, `shard:I/N`, `files:a.json`,
`match:REGEX`, a '+' union, a row of SLICES) plus two terms on the start of the file name (case-insensitive):

    startswith:out,sourcemap     cases whose file name starts with one of the comma-separated strings
    except:out,sourcemap         every other case

The cases split in two: the generated out/outDir/sourcemap/mapRoot/sourceRoot matrix (228 files, #1286) and the rest
(88 files, #1285: resolution, references, declarations*, privacyCheck*, nodeModules*, jsFileCompilation*, decorator
metadata, rootDirectory and the other hand-written scenarios). Another range is a new row, or a module of your own
with `--slices-module NAME`.
"""

import re

import ts_compiler_slices as base

MATRIX_STARTS = "out,sourcemap,maproot,sourceroot"

SLICES = {
    "core": f"except:{MATRIX_STARTS}",  # #1285
    "matrix": f"startswith:{MATRIX_STARTS}",  # #1286
    "first20": "first:20",
    "all": "match:.",
}

TITLE_RE = re.compile(
    r"^projects tests (Compiling project for .*?: testcase (tests/cases/project/\S+?\.json) \(@module: (\w+)\)) (Correct .*)$"
)


class Case:
    """One JSON test case of the project runner; `key` (what `match:` searches) is its file name, `tests` the full titles."""

    def __init__(self, file, describes):
        self.file = file
        self.base = file.rsplit("/", 1)[-1]
        self.key = self.base
        self.describes = describes
        self.tests = [t for d in describes.values() for t in d]

    def __repr__(self):
        return f"Case({self.key!r})"


def cases_from_titles(entries):
    """Group `(fullTitle, title)` pairs of the dry run into JSON cases, keeping the runner's order."""
    out = {}
    for full, title in entries:
        m = TITLE_RE.match(full)
        if not m or not full.endswith(title):
            continue
        describe = out.setdefault(m.group(2), {}).setdefault(m.group(1), [])
        if full not in describe:
            describe.append(full)
    return [Case(f, d) for f, d in out.items()]


def name_starts(arg):
    starts = tuple(s.strip().lower() for s in arg.split(",") if s.strip())
    if not starts:
        raise ValueError("startswith:/except: need at least one prefix")
    return lambda name: name.lower().startswith(starts)


def select(spec, cases, table=None):
    """Cases of `cases` chosen by `spec`, in enumeration order, without duplicates."""
    table = SLICES if table is None else table
    chosen = set()
    position = {id(c): i for i, c in enumerate(cases)}
    for term in base.flat_terms(spec, table):
        kind, _, arg = term.partition(":")
        if kind in ("startswith", "except"):
            ok = name_starts(arg)
            chosen.update(
                i for i, c in enumerate(cases) if ok(c.base) == (kind == "startswith")
            )
        else:
            chosen.update(position[id(c)] for c in base.select(term, cases, {}))
    return [cases[i] for i in sorted(chosen)]


batches = base.batches


def grep_for(batch):
    """A mocha --grep regex selecting exactly the tests of the JSON files of `batch`."""
    names = "|".join(base.js_escape(c.file) for c in batch)
    return (
        f"^projects tests Compiling project for .*: testcase (?:{names}) \\(@module: "
    )
