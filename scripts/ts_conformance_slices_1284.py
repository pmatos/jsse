"""Slice rows of TypeScript's `conformance` runner for scripts/run-ts-conformance-1284-oracle.py (issue #1284).

Suites, `dir:` terms and the pin lookup are those of scripts/ts_conformance_slices_1275.py; this module only adds the
rows (`--slices-module ts_conformance_slices_1284`). Issue #1284 is the directories async, asyncGenerators, generators,
enums, constEnums, interfaces and Symbols plus the two files at the root of tests/cases/conformance
(fixSignatureCaching.ts, inferFromBindingPattern.ts), cut into four chunks run one after the other so that a crashed or
interrupted one loses nothing. OWNERS assigns every top-level entry of tests/cases/conformance to the issue that runs
it, which the oracle's --check-partition compares with the checkout and with the buckets of ts-suite/oracle.json.
"""

GROUP_DIRS = (
    "async",
    "asyncGenerators",
    "generators",
    "enums",
    "constEnums",
    "interfaces",
    "Symbols",
)
ROOT_FILES = ("fixSignatureCaching.ts", "inferFromBindingPattern.ts")
ASYNC_ROOT_FILE = "asyncFunctionDeclarationParameterEvaluation.ts"

CHUNKS = {
    "as1": "dir:async/es5",
    "as2": "dir:async/es2017,async/es6",
    "if": "dir:interfaces",
    "mix": "dir:asyncGenerators,generators,enums,constEnums,Symbols+files:"
    + ",".join((ASYNC_ROOT_FILE, *ROOT_FILES)),
}

SLICES = {
    "issue1284": "dir:" + ",".join(GROUP_DIRS) + "+files:" + ",".join(ROOT_FILES),
    **{d: f"dir:{d}" for d in GROUP_DIRS},
    **CHUNKS,
}

_ES = [f"es{y}" for y in range(2016, 2026)]
OWNERS = {
    1274: ["types"],
    1275: ["expressions"],
    1276: ["classes", "decorators", "esDecorators", "override"],
    1277: ["es6"],
    1278: ["es5", "es7", *_ES, "esnext"],
    1279: ["statements", "controlFlow", "functions"],
    1280: ["parser", "scanner", "directives", "pedantic", "references"],
    1281: ["declarationEmit", "emitter", "ambient", "additionalChecks"],
    1282: ["jsx", "jsdoc", "salsa", "typings"],
    1283: [
        "moduleResolution",
        "node",
        "externalModules",
        "internalModules",
        "importAssertion",
        "importAttributes",
        "importDefer",
        "dynamicImport",
        "nonjsExtensions",
    ],
    1284: [*GROUP_DIRS, *ROOT_FILES],
}
