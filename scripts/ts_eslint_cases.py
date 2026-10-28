"""Cases for `hereby run-eslint-rules-tests`, TypeScript's custom ESLint rule tests (issue #1294).

Used by run-ts-mocha-oracle.py. The task is `runConsoleTests("scripts/eslint/tests", ...)`: mocha runs
`scripts/eslint/tests/*.cjs`, whose RuleTester (typescript-eslint's, over ESLint 10 and the TypeScript parser) lints
each snippet with the rule under test. The cases run the real hereby CLI with the checkout's Herebyfile in a work
directory, so the task's flags, its reporters, the exit status and `.failed-tests` are diffed between Node and jsse.

  real     the six rule test files of the checkout, with each reporter and each --tests filter;
  extra    scripts/ts-eslint-fixtures/extra-rules.test.cjs added to them: rules and RuleTester features the
           suite leaves out (fixers, `output`, `filename`, options, TypeScript-only syntax);
  broken   scripts/ts-eslint-fixtures/broken.test.cjs: every way RuleTester reports a failing case, and the
           --failed re-run once ESLINT_FIXED=1 repairs them.

Fixtures are copied into a private `scripts/eslint/tests` that otherwise links the checkout's files.
"""

import os
import shutil

from ts_runner_cli_cases import Case

TASK = "run-eslint-rules-tests"
FIXTURES = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "ts-eslint-fixtures"
)
NO_COLOR = ["--no-colors"]
RULES = "no-in-operator|jsdoc-format|thrower|no-keywords-schema|parse-error"


def eslint(name, flags, specs=(), env=None, then=None, must=()):
    return Case(
        name,
        [TASK, *flags],
        list(specs),
        None,
        dict(env or {}),
        "eslint",
        None,
        then,
        tuple(must),
    )


def eslint_cases():
    extra, broken = "extra-rules.test.cjs", "broken.test.cjs"
    spec = ["--reporter=spec", *NO_COLOR]
    passing = ("stdout", r"79 passing")
    ok, red = ("exit", r"^0$"), ("exit", r"^1$")
    first = ["--reporter=dot", *NO_COLOR]
    return [
        eslint("eslint-default", [], must=[("stdout", r"completed: 8 tests")]),
        eslint("eslint-no-color", ["--no-color"], must=[passing]),
        eslint("eslint-spec", spec, must=[passing]),
        eslint(
            "eslint-tap",
            ["--reporter=tap", *NO_COLOR],
            must=[("stdout", r"^# pass 79")],
        ),
        eslint("eslint-dot", first, must=[passing]),
        eslint(
            "eslint-json",
            ["--reporter", "json", *NO_COLOR],
            must=[("stdout", r'"passes": 79')],
        ),
        eslint(
            "eslint-tests-one-rule",
            ["--tests=no-in-operator", *spec],
            must=[("stdout", r"2 passing")],
        ),
        eslint("eslint-tests-alternation", ["-t", "no-keywords|js-extensions", *spec]),
        eslint("eslint-tests-none", ["--tests=no such rule", *spec]),
        eslint("eslint-extra", spec, [extra], must=[ok, ("stdout", r"^ +\d+ passing")]),
        eslint(
            "eslint-extra-fivemat", [], [extra], must=[ok, ("stdout", r"completed: ")]
        ),
        eslint(
            "eslint-extra-only",
            [
                "--tests=jsdoc-format|no-direct-import|no-keywords-options|no-in-operator-ts",
                *spec,
            ],
            [extra],
            must=[ok, ("stdout", r"no-direct-import")],
        ),
        eslint(
            "eslint-broken-spec",
            [f"--tests={RULES}", *spec],
            [broken],
            must=[
                red,
                ("stdout", r"failing"),
                ("file .failed-tests", r"no-in-operator"),
            ],
        ),
        eslint(
            "eslint-broken-fivemat",
            [f"--tests={RULES}", "--no-color"],
            [broken],
            must=[red, ("stdout", r"failed:")],
        ),
        eslint(
            "eslint-broken-tap",
            [f"--tests={RULES}", "--reporter=tap", *NO_COLOR],
            [broken],
            must=[red, ("stdout", r"^not ok ")],
        ),
        eslint(
            "eslint-broken-rerun-fixed",
            [f"--tests={RULES}", *first],
            [broken],
            then=([TASK, "--failed", *spec], {"ESLINT_FIXED": "1"}),
            must=[("then stdout", r"passing")],
        ),
        eslint(
            "eslint-broken-rerun-unfixed",
            [f"--tests={RULES}", *first],
            [broken],
            then=([TASK, "--failed", *first], {}),
            must=[("then file .failed-tests", r"no-in-operator")],
        ),
        eslint(
            "eslint-broken-fixed",
            [f"--tests={RULES}", *spec],
            [broken],
            {"ESLINT_FIXED": "1"},
            must=[ok, ("stdout", r"passing")],
        ),
    ]


def link_scripts(cwd, ts_dir, specs):
    """A `scripts` directory of links into the checkout whose eslint/tests also holds the fixtures."""
    dest = os.path.join(cwd, "scripts")
    os.remove(dest)
    src = os.path.join(ts_dir, "scripts")
    tests = os.path.join(src, "eslint", "tests")
    os.makedirs(os.path.join(dest, "eslint", "tests"))
    for base, sub in (
        (src, ""),
        (os.path.join(src, "eslint"), "eslint"),
        (tests, "eslint/tests"),
    ):
        for name in os.listdir(base):
            target = os.path.join(dest, sub, name)
            if not os.path.exists(target):
                os.symlink(os.path.join(base, name), target)
    for spec in specs:
        shutil.copy(
            os.path.join(FIXTURES, spec), os.path.join(dest, "eslint", "tests", spec)
        )
