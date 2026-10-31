"""Slice rows of TypeScript's `conformance` runner for the `es6` directory (issue #1277).

Rows for `--slices-module ts_conformance_slices_1277` of scripts/run-ts-compiler-oracle.py, over the suites and `dir:`
terms of scripts/ts_conformance_slices.py. `es6` (tests/cases/conformance/es6: 1315 suites, the 7890 tests pinned in
ts-suite/oracle.json) is cut into five chunks of 230 to 300 suites, one oracle run each, so that a crashed or interrupted
chunk loses nothing.
"""

CHUNK_DIRS = {
    "es1": (
        "Symbols arrowFunction binaryAndOctalIntegerLiteral classDeclaration classExpressions decorators "
        "defaultParameters"
    ).split(),
    "es2": (
        "computedProperties for-ofStatements functionDeclarations functionExpressions functionPropertyAssignments "
        "memberFunctionDeclarations"
    ).split(),
    "es3": (
        "destructuring modules moduleExportsAmd moduleExportsCommonjs moduleExportsSystem moduleExportsUmd newTarget "
        "propertyAccess restParameters"
    ).split(),
    "es4": "shorthandPropertyAssignment spread templates variableDeclarations".split(),
    "es5": "unicodeExtendedEscapes yieldExpressions".split(),
}
CHUNKS = {
    name: "dir:" + ",".join("es6/" + d for d in dirs)
    for name, dirs in CHUNK_DIRS.items()
}
SLICES = {"es6": "dir:es6", **CHUNKS}
