"""Cases of scripts/run-ts-lint-oracle.py (issue #1295): `hereby lint` and ESLint on the TypeScript checkout.

`hereby lint` is `node node_modules/eslint/bin/eslint --cache --cache-location ./.eslintcache
--report-unused-disable-directives --max-warnings 0 .` over the compiler and script sources (ESLint 10 and
typescript-eslint: type-aware rules for src/, a second program for scripts/, regexp and TypeScript's own rules of
scripts/eslint/rules). The whole repository is clean on Node, so a clean run alone proves little: the `ctl-*` cases
add the control files of scripts/ts-lint-fixtures to private copies and lint them (about thirty diagnostics of
every kind, `--fix` rewriting them, the cache file, the formatters); the `full` case lints everything.

A case is a list of steps run one after the other in one private tree (`eslint` runs the ESLint CLI with the lint
task's flags unless `plain`, `hereby` runs the hereby CLI). The tree is a light copy (src, scripts and the root
files; node_modules and the rest symlinked) except for `full`, which copies the whole checkout.
"""

from dataclasses import dataclass
from fnmatch import fnmatchcase

LINT_FLAGS = ["--report-unused-disable-directives", "--max-warnings", "0"]
SRC = "src/compiler/lintControl.ts"
MJS = "scripts/lintControl.mjs"
WARN = "scripts/lintWarn.mjs"
FIXTURES = {
    SRC: "lintControl.ts",
    MJS: "lintControl.mjs",
    WARN: "lintWarn.mjs",
}


@dataclass
class Step:
    tool: str
    args: list
    plain: bool = False


@dataclass
class Case:
    name: str
    steps: list
    controls: tuple = ()
    full: bool = False
    must: tuple = ()
    default: bool = True
    cheap: bool = False
    notes: str = ""


def lint(*args, plain=False):
    return Step("eslint", list(args), plain)


def hereby(*args):
    return Step("hereby", list(args))


def cases():
    both = (SRC, MJS)
    exit0, exit1 = ("exit", r"^0$"), ("exit", r"^1$")
    return [
        Case(
            "version",
            [lint("--version", plain=True)],
            must=(("stdout", r"^v10\."),),
            cheap=True,
        ),
        Case(
            "print-config-src",
            [lint("--print-config", "src/compiler/core.ts", plain=True)],
            must=(("stdout", r"local/argument-trivia"), exit0),
            cheap=True,
        ),
        Case(
            "print-config-scripts",
            [lint("--print-config", "scripts/build/utils.mjs", plain=True)],
            must=(("stdout", r"no-restricted-globals"), exit0),
            cheap=True,
        ),
        Case(
            "print-config-dts",
            [lint("--print-config", "src/lib/es5.d.ts", plain=True)],
            must=(("stdout", r"no-restricted-syntax"), exit0),
            cheap=True,
        ),
        Case(
            "ctl-scripts",
            [lint(MJS)],
            (MJS,),
            must=(
                exit1,
                ("stdout", r"no-new-wrappers"),
                ("stdout", r"object-shorthand"),
            ),
            cheap=True,
        ),
        Case(
            "ctl-scripts-fix",
            [lint("--fix", MJS), lint(MJS)],
            (MJS,),
            must=(
                exit1,
                ("file scripts/lintControl.mjs", r"const fixable"),
                ("stdout", r"no-eval"),
            ),
            cheap=True,
        ),
        Case(
            "ctl-scripts-formats",
            [
                lint("-f", f, MJS)
                for f in ("stylish", "json", "json-with-metadata", "html")
            ]
            + [lint("-f", "json", "-o", "report.json", MJS)],
            (MJS,),
            must=(
                exit1,
                ("stdout", r'"ruleId":"no-var"'),
                ("stdout", r"<html"),
                ("stdout", r'"rulesMeta"'),
                ("file report.json", r'"ruleId":"no-var"'),
            ),
            cheap=True,
        ),
        Case(
            "ctl-warning-only",
            [lint(WARN)],
            (WARN,),
            must=(exit1, ("stderr", r"ESLint found too many warnings \(maximum: 0\)")),
            cheap=True,
        ),
        Case(
            "ctl-src",
            [lint(SRC)],
            (SRC,),
            must=(
                exit1,
                ("stdout", r"local/argument-trivia"),
                ("stdout", r"no-unnecessary-type-assertion"),
                ("stdout", r"regexp/no-super-linear-backtracking"),
                ("stdout", r"Unused eslint-disable directive"),
            ),
        ),
        Case(
            "ctl-src-json",
            [lint("-f", "json", SRC)],
            (SRC,),
            must=(exit1, ("stdout", r'"fix":\{"range"')),
        ),
        Case(
            "ctl-src-fix",
            [lint("--fix", SRC), lint(SRC)],
            (SRC,),
            must=(
                exit1,
                ("file src/compiler/lintControl.ts", r"let oldStyle|const oldStyle"),
            ),
        ),
        Case(
            "ctl-both-cache",
            [lint("--cache", "--cache-location", ".eslintcache", *both)] * 2,
            both,
            must=(exit1, ("file .eslintcache", r"lintControl")),
        ),
        Case(
            "full",
            [hereby("lint"), hereby("lint")],
            full=True,
            default=False,
            must=(
                exit0,
                ("stdout", r"Completed lint in"),
                ("file .eslintcache", r"core\.ts"),
            ),
            notes="the whole repository; the second run reads the cache the first one wrote",
        ),
    ]


def select(names, table):
    """The cases `names` picks: none means every default case, `all` everything, `cheap` the cheap ones."""
    if not names:
        return [c for c in table if c.default]
    out = []
    for n in names:
        if n == "all":
            out += [c for c in table if c not in out]
        elif n == "cheap":
            out += [c for c in table if c.cheap and c not in out]
        else:
            hit = [c for c in table if c.name == n or fnmatchcase(c.name, n)]
            if not hit:
                raise SystemExit(f"no case matches {n!r}")
            out += [c for c in hit if c not in out]
    return out
