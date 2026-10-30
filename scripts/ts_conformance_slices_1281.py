"""Slices of TypeScript's `conformance` runner for issue #1281: declarationEmit, emitter, ambient, additionalChecks.

The suite parsing, `dir:` selection and pinned counts are those of scripts/ts_conformance_slices.py (issue #1276); this
module adds the rows of the #1281 group, so that `select` finds them. `ts-suite/oracle.json` pins the passing tests
per top-level directory: declarationEmit 138, emitter 102, ambient 132, additionalChecks 6 (378 tests, 63 suites).
"""

import ts_conformance_slices as base
from ts_conformance_slices import (  # noqa: F401
    PREFIX,
    ROOT,
    SUITE_PREFIX,
    Suite,
    batches,
    char_ranges,
    flat_terms,
    grep_for,
    suites_from_titles,
    top_dirs,
)

GROUP_1281 = ("declarationEmit", "emitter", "ambient", "additionalChecks")

SLICES = {d: f"dir:{d}" for d in GROUP_1281}
SLICES["issue1281"] = "dir:" + ",".join(GROUP_1281)


def select(spec, suites, table=None):
    """Suites of `suites` chosen by `spec`, in enumeration order, without duplicates."""
    return base.select(spec, suites, SLICES if table is None else table)
