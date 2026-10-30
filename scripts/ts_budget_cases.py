"""Cases of the TypeScript suite budget check run by run-ts-budget-oracle.py (issue #1297).

A case is one mocha process per engine over `built/local/run.js` of a TypeScript v6.0.3 checkout: the runners that
`test.config` enables and a `--grep` regular expression over the full test titles. The cases are sized so that every
runner of ts-suite/oracle.json (compiler, conformance, fourslash, fourslash-server, project, transpile and the unit
tests) is represented by a selection whose cost is dominated by what dominates the whole runner: loading the 13 MB
runner bundle, the first `Program` (lib.d.ts), then per-test work. Selections are by file-name prefix or suite title,
so they stay the same while the pinned tree does. `smoke` cases run on every PR that touches the engine; the others
run nightly.
"""

COMPILER = "^compiler tests compiler tests for tests/cases/compiler/"
CONFORMANCE = "^conformance tests conformance tests for tests/cases/conformance/"

# name -> (runners enabled in test.config, mocha --grep regex, smoke, what the case stands for)
CASES = {
    "bundle-load": (
        ["unittest"],
        r"^unittests:: debugDeprecation",
        True,
        "6 unit tests: the process is the load of the 13 MB runner bundle",
    ),
    "transpile": (
        ["transpile"],
        r"^transpile tests ",
        True,
        "the whole transpile runner (37 tests, no Program, no lib)",
    ),
    "compiler-first": (
        ["compiler"],
        COMPILER + r"a[a-b]",
        True,
        "10 compiler suites (60 tests): bundle load, first Program, lib parse",
    ),
    "compiler-366": (
        ["compiler"],
        COMPILER + r"(a[a-f]|b[a-c])",
        False,
        "366 tests of the compiler suites a[a-f]* and b[a-c]*: type checking, emit and baseline diffing",
    ),
    "conformance-interfaces": (
        ["compiler"],
        CONFORMANCE + r"interfaces/",
        False,
        "396 conformance tests of the interfaces directory",
    ),
    "conformance-generators": (
        ["compiler"],
        CONFORMANCE + r"generators/",
        False,
        "102 conformance tests of the generators directory (downlevel emit)",
    ),
    "project-out": (
        ["project"],
        r"^projects tests Compiling project for [a-z]*out",
        False,
        "200 project-runner tests (out*, amd and commonjs compiles)",
    ),
    "fourslash-add": (
        ["fourslash"],
        r"^fourslash tests tests/cases/fourslash/a[a-d]",
        False,
        "language-service tests of the fourslash files a[a-d]*",
    ),
    "fourslash-server": (
        ["fourslash-server"],
        r"^fourslash-server tests ",
        False,
        "the 488 tsserver-protocol fourslash tests",
    ),
    "tsserver-sourcemap": (
        ["unittest"],
        r"^unittests:: tsserver:: .*projectReferencesSourcemap",
        False,
        "273 tests: the heaviest tsserver file (projects, source maps, many Programs)",
    ),
    "tscwatch": (
        ["unittest"],
        r"^unittests:: tscWatch::",
        False,
        "375 tscWatch tests on the virtual watch host",
    ),
    "tsbuild": (
        ["unittest"],
        r"^unittests:: tsbuild::",
        False,
        "359 solution-builder tests",
    ),
    "evaluation": (
        ["unittest"],
        r"^unittests:: evaluation::",
        False,
        "350 tests that compile and then evaluate emitted code",
    ),
    "services": (
        ["unittest"],
        r"^unittests:: services::",
        False,
        "1531 language-service unit tests",
    ),
}
SMOKE = tuple(name for name, case in CASES.items() if case[2])
