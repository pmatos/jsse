"""Slice rows of TypeScript's `conformance` runner for scripts/run-ts-conformance-es-oracle.py (issue #1278).

The suite enumeration, `dir:` selection and the pins of ts-suite/oracle.json are those of
scripts/ts_conformance_slices_1275.py (`--slices-module` adds the rows below to its table). Issue #1278 is the
directories es5, es7, es2016 to es2025 and esnext of tests/cases/conformance (226 suites, the 1356 tests pinned in
oracle.json), cut into four chunks that run one after the other so a crashed chunk does not lose the others.
"""

YEARS = [f"es{y}" for y in range(2016, 2026)]
GROUP_1278 = ["es5", "es7", *YEARS, "esnext"]

CHUNK_DIRS = {
    "es-a": ["es5", "es7"],
    "es-b": YEARS[:4],
    "es-c": YEARS[4:6],
    "es-d": [*YEARS[6:], "esnext"],
}
CHUNKS = {name: "dir:" + ",".join(dirs) for name, dirs in CHUNK_DIRS.items()}
SLICES = {
    "issue1278": "dir:" + ",".join(GROUP_1278),
    **{d: f"dir:{d}" for d in GROUP_1278},
    **CHUNKS,
}
