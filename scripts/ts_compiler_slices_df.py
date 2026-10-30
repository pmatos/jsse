"""Slice rows for the compiler runner files D-F (issue #1270): `--slices-module ts_compiler_slices_df`.

`d-f` is the whole range (1693 suites, 10158 tests, the D, E and F buckets of ts-suite/oracle.json). CHUNKS cuts it into
pieces of at most ~275 suites that one oracle run finishes in about twenty minutes; they partition `d-f` exactly
(scripts/test_ts_compiler_slices_df.py). A `match:` term matches the suite title, which contains the path
`tests/cases/compiler/<file>`.
"""

CHUNKS = {
    "d1": "match:(?i)compiler/d[aiotuy]",
    "d2": "match:(?i)compiler/(de[^c]|dec[^l])",
    "d3": "match:(?i)compiler/decl(?!arationemit)",
    "d4": "match:(?i)compiler/declarationemit[a-d]",
    "d5": "match:(?i)compiler/declarationemit[e-m]",
    "d6": "match:(?i)compiler/declarationemit[n-z]",
    "e1": "match:(?i)compiler/e[a-n]",
    "e2": "match:(?i)compiler/e[o-r]",
    "e3": "match:(?i)compiler/es[2-5]",
    "e4": "match:(?i)compiler/es(?![2-5])",
    "e5": "match:(?i)compiler/(ev|ex[^p])",
    "e6": "match:(?i)compiler/exp",
    "f": "prefix:F",
}

SLICES = {"d-f": "prefix:D-F", **CHUNKS}
