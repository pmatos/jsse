"""Groups of TypeScript's top-level unit-test suites for run-ts-unittests-oracle.py (issue #1259).

`src/testRunner/unittests/*.ts` register the mocha suites of 26 groups (#1259); the 16 `services-*` groups
(#1261) cover `unittests/services/*.ts` and `unittests/services/extract/*.ts`, 1531 tests. The other subdirectories
(config and sys: run-ts-unittests-config-sys-oracle.py; evaluation, tsbuild, tscWatch, tsc, tsserver: their own
tickets) are not in SUBDIRS. A group is one file: its
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

S = r"^unittests:: services:: "
GROUPS += [
    Group("services-cancellable", S + r"cancellableLanguageServiceOperations ", 5, 6),
    Group("services-colorization", S + r"Colorization ", 38, 7),
    Group("services-convertToAsyncFunction", S + r"convertToAsyncFunction:: ", 264, 4),
    Group("services-documentRegistry", S + r"DocumentRegistry ", 4, 6),
    Group("services-extractConstants", S + r"extract:: extractConstants ", 92, 5),
    Group("services-extractFunctions", S + r"extract:: extractFunctions ", 122, 5),
    Group("services-extractRanges", S + r"extract:: extractRanges ", 50, 5),
    Group("services-extractSymbolWalker", S + r"extract:: Symbol Walker ", 1, 5),
    Group("services-hostNewLineSupport", S + r"hostNewLineSupport ", 3, 6),
    Group("services-languageService", S + r"languageService:: ", 6, 6),
    Group("services-organizeImports", S + r"organizeImports:: ", 91, 6),
    Group("services-patternMatcher", S + r"PatternMatcher ", 75, 7),
    Group("services-preProcessFile", S + r"PreProcessFile: ", 41, 7),
    Group("services-textChanges", S + r"textChanges ", 84, 6),
    Group("services-transpile", S + r"Transpile ", 654, 7),
    Group("services-utilities", S + r"utilities ", 1, 6),
]

DESCRIBE = re.compile(r"""^describe(?:\.\w+)?\(\s*["'`]([^"'`]+)["'`]""", re.M)


SUBDIRS = ("", "services", os.path.join("services", "extract"))


def top_level_titles(ts_dir):
    """The first-level describe titles of every `.ts` file of the directories in SUBDIRS under
    `unittests/` (the other subdirectories belong to other oracles), by file name."""
    found = {}
    for sub in SUBDIRS:
        base = os.path.join(ts_dir, "src", "testRunner", "unittests", sub)
        if not os.path.isdir(base):
            continue
        for name in sorted(os.listdir(base)):
            path = os.path.join(base, name)
            if name.endswith(".ts") and os.path.isfile(path):
                with open(path, encoding="utf-8") as f:
                    titles = DESCRIBE.findall(f.read())
                if titles:
                    found[os.path.join(sub, name)] = titles
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
