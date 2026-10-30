"""Slices of TypeScript's `conformance` runner for the jsx, jsdoc, salsa and typings directories (issue #1282).

The suite model, the `dir:` term and the compiler terms (`first:N`, `prefix:`, `shard:I/N`, `files:`, `match:`) are those
of scripts/ts_conformance_slices.py (#1276); this module only adds the rows of the #1282 group:

    jsx, jsdoc, salsa, typings   one top-level directory of tests/cases/conformance each
    issue1282                    the four (ts-suite/oracle.json pins 1392 + 2382 + 1158 + 54 = 4986 passing tests)
"""

import ts_conformance_slices as base

TESTS_PER_SUITE = 6  # errors, module resolution trace, sourcemap record, JS, sourcemap, types and symbols
GROUP_1282 = ("jsx", "jsdoc", "salsa", "typings")

SLICES = {d: f"dir:{d}" for d in GROUP_1282}
SLICES["issue1282"] = "dir:" + ",".join(GROUP_1282)

PREFIX = base.PREFIX
SUITE_PREFIX = base.SUITE_PREFIX
Suite = base.Suite
suites_from_titles = base.suites_from_titles
top_dirs = base.top_dirs
batches = base.batches
flat_terms = base.flat_terms
char_ranges = base.char_ranges
grep_for = base.grep_for


def select(spec, suites, table=None):
    """Suites of `suites` chosen by `spec`, in enumeration order, without duplicates."""
    return base.select(spec, suites, SLICES if table is None else table)
