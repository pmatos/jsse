#!/usr/bin/env python3
"""Run TypeScript's `tsc` CLI unit tests (unittests/tsc/*.ts) on Node and on jsse --node and diff them per test (issue #1263).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-unittests-tsc-oracle.py [options] [group ...]

The machinery is scripts/run-ts-unittests-config-sys-oracle.py (same options, same per-title comparison, same
checkout requirements, --record, --list) with the groups of scripts/ts_unittest_tsc_groups.py (one per file of
unittests/tsc), the baseline ts-suite/unittests-tsc.json and the pin of the `tsc` suite in ts-suite/oracle.json.
"""

import importlib.util
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from ts_unittest_tsc_groups import GROUPS  # noqa: E402

spec = importlib.util.spec_from_file_location(
    "run_ts_unittests_base", os.path.join(HERE, "run-ts-unittests-config-sys-oracle.py")
)
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)

base.GROUPS = GROUPS
base.BASELINE = os.path.join(os.path.dirname(HERE), "ts-suite", "unittests-tsc.json")
base.SUITE_OF = {"tsc": "tsc-"}

if __name__ == "__main__":
    sys.exit(base.main())
