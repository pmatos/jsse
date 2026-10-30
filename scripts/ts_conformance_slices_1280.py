"""Slices of TypeScript's `conformance` runner for scripts/run-ts-conformance-1280-oracle.py (issue #1280).

Everything of scripts/ts_conformance_slices.py (suite titles, `dir:`, `first:N`, `shard:I/N`, `files:`, `match:`) plus
the issue's directories parser, scanner, directives, pedantic and references (5832 tests in 972 suites) and

    only:parser/ecmascript5   suites whose test file lies directly in that directory, not in a subdirectory

which cuts the 796 suites of parser/ecmascript5 into chunks without leaving its top-level files out.
"""

import ts_conformance_slices as conf
from ts_conformance_slices import *  # noqa: F401,F403  (the API the oracle looks up on its slices module)

GROUP = ("directives", "pedantic", "references", "scanner", "parser")

E5 = "parser/ecmascript5/"


def e5(names):
    return "dir:" + ",".join(E5 + d for d in names.split())


CHUNK_PARTS = {
    "cf1": [
        "dir:directives,pedantic,references,scanner",
        "dir:parser/ecmascript2018,parser/ecmascript2021,parser/ecmascript3,parser/ecmascript6",
    ],
    "cf2": [
        "only:parser/ecmascript5",
        e5(
            "Accessors ArrayLiteralExpressions ArrowFunctionExpressions AutomaticSemicolonInsertion "
            "CatchClauses ClassDeclarations ComputedPropertyNames ConstructorDeclarations EnumDeclarations"
        ),
    ],
    "cf3": [
        e5(
            "ErrorRecovery ExportAssignments Expressions FunctionDeclarations Fuzz Generics "
            "IndexMemberDeclarations"
        )
    ],
    "cf4": [
        e5(
            "IndexSignatures InterfaceDeclarations MemberAccessorDeclarations MemberFunctionDeclarations "
            "MemberVariableDeclarations MethodSignatures MissingTokens ModuleDeclarations ObjectLiterals "
            "ObjectTypes ParameterLists PropertyAssignments PropertySignatures Protected RealWorld "
            "RegressionTests"
        )
    ],
    "cf5": [
        e5(
            "RegularExpressions SkippedTokens Statements StrictMode SuperExpressions Symbols TupleTypes "
            "Types VariableDeclarations"
        )
    ],
}
CHUNKS = {name: "+".join(parts) for name, parts in CHUNK_PARTS.items()}

SLICES = {d: f"dir:{d}" for d in GROUP}
SLICES["issue1280"] = "dir:" + ",".join(GROUP)
SLICES.update(CHUNKS)


def select(spec, suites, table=None):
    """Suites chosen by `spec`: `only:` terms here, every other term by scripts/ts_conformance_slices.py."""
    table = SLICES if table is None else table
    chosen = set()
    for term in conf.flat_terms(spec, table):
        kind, _, arg = term.partition(":")
        if kind == "only" and arg:
            where = arg.strip().strip("/")
            chosen.update(
                i
                for i, s in enumerate(suites)
                if "/" in s.rel and s.rel.rsplit("/", 1)[0] == where
            )
        else:
            keys = {s.key for s in conf.select(term, suites, {})}
            chosen.update(i for i, s in enumerate(suites) if s.key in keys)
    return [suites[i] for i in sorted(chosen)]
