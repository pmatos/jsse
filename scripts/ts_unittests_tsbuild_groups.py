"""Rows of the 31 `unittests/tsbuild/*.ts` groups for ts_unittests_groups.py (issue #1264, 410 tests).

Each row is (name, pattern, tests, batch) like `ts_unittests_groups.Group`; `tests` is the count Node 26 reports on
TypeScript v6.0.3 and the rows add up to the `tsbuild` suite of `ts-suite/oracle.json`. `tsbuildWatch` (a
separate directory and ticket) is not selected: every pattern needs `tsbuild:: ` or `tsbuild - `. The batches
(20 to 23, about 100 tests each) balance the four mocha processes: the `noEmit`, `sample` and `noEmitOnError`
suites build the solution after many edits and take the longest on jsse.
"""

B = r"^unittests:: tsbuild:: "
C = r"^unittests:: tsbuild - "

ROWS = [
    ("tsbuild-amdModulesWithOut", B + r"outFile:: amdModulesWithOut:: ", 5, 23),
    ("tsbuild-clean", C + r"clean:: ", 3, 23),
    ("tsbuild-commandLine", B + r"commandLine:: ", 33, 22),
    ("tsbuild-configFileErrors", B + r"configFileErrors:: ", 5, 23),
    ("tsbuild-configFileExtends", B + r"configFileExtends:: ", 2, 23),
    ("tsbuild-containerOnlyReferenced", B + r"when containerOnly project ", 4, 23),
    ("tsbuild-declarationEmit", B + r"declarationEmit:: ", 11, 20),
    ("tsbuild-demo", B + r"on demo:: project ", 3, 23),
    (
        "tsbuild-emitDeclarationOnly",
        B + r"on project with emitDeclarationOnly:: ",
        6,
        23,
    ),
    ("tsbuild-emptyFiles", B + r"emptyFiles:: ", 2, 23),
    ("tsbuild-exitCodeOnBogusFile", B + r"exitCodeOnBogusFile:: ", 1, 23),
    ("tsbuild-extends", B + r"extends:: ", 2, 23),
    ("tsbuild-fileDelete", B + r"fileDelete:: ", 8, 23),
    ("tsbuild-graphOrdering", B + r"graphOrdering:: ", 4, 23),
    (
        "tsbuild-inferredTypeFromTransitiveModule",
        B + r"inferredTypeFromTransitiveModule:: ",
        6,
        23,
    ),
    ("tsbuild-javascriptProjectEmit", B + r"javascriptProjectEmit:: ", 2, 23),
    ("tsbuild-lateBoundSymbol", B + r"lateBoundSymbol:: ", 2, 23),
    ("tsbuild-libraryResolution", B + r"libraryResolution:: ", 2, 23),
    ("tsbuild-moduleResolution", B + r"moduleResolution:: ", 9, 20),
    ("tsbuild-moduleSpecifiers", B + r"moduleSpecifiers:: ", 2, 23),
    ("tsbuild-noCheck", B + r"noCheck:: ", 24, 22),
    ("tsbuild-noEmit", B + r"noEmit:: ", 88, 20),
    ("tsbuild-noEmitOnError", C + r"with noEmitOnError:: ", 48, 21),
    ("tsbuild-outFile", B + r"outFile:: (?!amdModulesWithOut:: )", 18, 22),
    ("tsbuild-outputPaths", B + r"outputPaths:: ", 15, 20),
    ("tsbuild-publicApi", B + r"PublicAPI:: ", 1, 23),
    ("tsbuild-resolveJsonModule", B + r"with resolveJsonModule:: ", 26, 22),
    ("tsbuild-rootDirInParent", B + r"projectReferenceWithRootDirInParent:: ", 9, 20),
    ("tsbuild-roots", B + r"roots:: ", 12, 20),
    ("tsbuild-sample", B + r"on 'sample1' project ", 54, 21),
    ("tsbuild-transitiveReferences", B + r"transitiveReferences:: ", 3, 23),
]
