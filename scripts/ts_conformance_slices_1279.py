"""Slice rows of TypeScript's `conformance` runner for the statements, controlFlow and functions directories (issue #1279).

Suites, `dir:` terms and the pin lookup are those of scripts/ts_conformance_slices_1275.py; this module only adds the
rows (`--slices-module ts_conformance_slices_1279`). The group is cut into chunks so that a crashed or interrupted one
loses nothing: `st1` (statements/VariableStatements), `st2` (the for, for-in, for-of and for-await-of statements),
`st3` (the other statements directories), `cf` (controlFlow) and `fn` (functions). `issue1279` is the three directories
whole; ts-suite/oracle.json pins statements 3150, controlFlow 348 and functions 150 passing tests.
"""

DIRS = ("statements", "controlFlow", "functions")

STATEMENTS_ST1 = ("VariableStatements",)
STATEMENTS_ST2 = (
    "for-await-ofStatements",
    "for-inStatements",
    "for-ofStatements",
    "forStatements",
)
STATEMENTS_ST3 = (
    "breakStatements",
    "continueStatements",
    "ifDoWhileStatements",
    "labeledStatements",
    "returnStatements",
    "switchStatements",
    "throwStatements",
    "tryStatements",
    "withStatements",
)
STATEMENTS = STATEMENTS_ST1 + STATEMENTS_ST2 + STATEMENTS_ST3


def _dirs(parent, names):
    return "dir:" + ",".join(f"{parent}/{n}" for n in names)


CHUNKS = {
    "st1": _dirs("statements", STATEMENTS_ST1),
    "st2": _dirs("statements", STATEMENTS_ST2),
    "st3": _dirs("statements", STATEMENTS_ST3),
    "cf": "dir:controlFlow",
    "fn": "dir:functions",
}

SLICES = {
    "issue1279": "dir:" + ",".join(DIRS),
    "statements": "dir:statements",
    "controlFlow": "dir:controlFlow",
    "functions": "dir:functions",
    **CHUNKS,
}
