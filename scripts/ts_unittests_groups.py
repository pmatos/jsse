"""Groups of TypeScript's top-level unit-test suites for run-ts-unittests-oracle.py (issue #1259).

`src/testRunner/unittests/*.ts` (not its subdirectories, which the config, evaluation, services, sys, tsbuild,
tscWatch, tsc and tsserver tickets own) register the mocha suites of 26 groups. A group is one file: its
`pattern` is the mocha `-g` regular expression selecting that file's tests (a test's full title is its
`describe` titles and its own title joined by spaces, so every pattern is anchored on the file's first
describe title plus a space), and `tests` is the count Node 26 reports on TypeScript v6.0.3. Every mocha process pays jsse's 80 s start-up (parsing and
running the 13 MB `run.js` bundle), so the oracle runs the groups of one `batch` in a single process (the pattern is
the alternation of theirs) and splits the report by group; the batches balance the slow suites (`canWatch` and
`programApi` take about two minutes of test time on jsse) over four processes.

`check_groups()` is the drift guard: every top-level `describe("...")` in the checkout must be matched by exactly
one group, so a new or renamed suite cannot be left out of the oracle unnoticed.
"""

import os
import re
from typing import NamedTuple


class Group(NamedTuple):
    name: str
    pattern: str
    tests: int
    batch: int = 3


GROUPS = [
    Group("asserts", r"^unittests:: assert ", 2),
    Group("base64", r"^unittests:: base64 ", 1),
    Group("builder", r"^unittests:: builder ", 3),
    Group("canWatch", r"^unittests:: canWatch:: ", 92, 0),
    Group("comments", r"^comment parsing ", 3),
    Group("compilerCore", r"^unittests:: compilerCore ", 11),
    Group("convertToBase64", r"^unittests:: convertToBase64 ", 5),
    Group("customTransforms", r"^unittests:: customTransforms ", 7),
    Group("debugDeprecation", r"^unittests:: debugDeprecation ", 6),
    Group(
        "diagnosticCollection", r"^unittests:: internalApi:: diagnosticCollection ", 5
    ),
    Group("factory", r"^unittests:: FactoryAPI ", 3),
    Group("incrementalParser", r"^unittests:: incrementalParser:: ", 153, 2),
    Group("jsDocParsing", r"^unittests:: JSDocParsing ", 125),
    Group("jsonParserRecovery", r"^unittests:: jsonParserRecovery ", 5),
    Group("moduleResolution", r"^unittests:: moduleResolution:: ", 39),
    Group("parsePseudoBigInt", r"^unittests:: BigInt literal base conversions ", 5, 2),
    Group("paths", r"^unittests:: core paths ", 14),
    Group("printer", r"^unittests:: PrinterAPI ", 43),
    Group("programApi", r"^unittests:: (programApi:: |Program\.)", 14, 1),
    Group("publicApi", r"^unittests:: Public APIs", 31),
    Group("regExpScannerRecovery", r"^unittests:: regExpScannerRecovery ", 984),
    Group("reuseProgramStructure", r"^unittests:: reuseProgramStructure:: ", 38, 2),
    Group("semver", r"^unittests:: semver ", 692),
    Group("skipJSDocParsing", r"^unittests:: skipJSDocParsing ", 24),
    Group("transform", r"^unittests:: TransformAPI ", 28),
    Group(
        "typeParameterIsPossiblyReferenced",
        r"^unittests :: internalApi :: typeParameterIsPossiblyReferenced ",
        1,
    ),
]

DESCRIBE = re.compile(r'^describe\("([^"]+)"', re.M)


def top_level_titles(ts_dir):
    """The first-level describe titles of every `unittests/*.ts` file, by file name."""
    base = os.path.join(ts_dir, "src", "testRunner", "unittests")
    found = {}
    for name in sorted(os.listdir(base)):
        path = os.path.join(base, name)
        if name.endswith(".ts") and os.path.isfile(path):
            with open(path, encoding="utf-8") as f:
                titles = DESCRIBE.findall(f.read())
            if titles:
                found[name] = titles
    return found


def matching(title):
    """The groups whose pattern selects a test under the describe `title`."""
    return [g.name for g in GROUPS if re.search(g.pattern, title + " ")]


def check_groups(ts_dir):
    """Problems (a list of strings) between the groups and the checkout's top-level suites."""
    problems = []
    for file, titles in top_level_titles(ts_dir).items():
        for title in titles:
            hits = matching(title)
            if len(hits) != 1:
                problems.append(
                    f"{file}: describe({title!r}) matches groups {hits or 'none'}"
                )
    return problems
