#!/usr/bin/env python3
"""Run TypeScript's tscWatch and tsbuildWatch unit tests on Node and on jsse --node and diff them per test (issue #1265).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-unittests-watch-oracle.py [options] [group ...]

The machinery is scripts/run-ts-unittests-config-sys-oracle.py (same options, same per-title comparison, same
checkout requirements, --record, --list) with the groups of scripts/ts_unittest_watch_groups.py (one per file of
unittests/tscWatch and unittests/tsbuildWatch, `tscWatch-*` and `tsbuildWatch-*`), the baseline
ts-suite/unittests-watch.json and the pins of the `tscWatch` and `tsbuildWatch` suites in ts-suite/oracle.json.
The tests run on a fake watch host (virtual timers and file events, no real fs.watch): they are CPU-bound, so the
run is split by group (a group is a glob argument, e.g. 'tscWatch-programUpdates' or 'tsbuildWatch-*').
"""

import importlib.util
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from ts_unittest_watch_groups import GROUPS  # noqa: E402

spec = importlib.util.spec_from_file_location(
    "run_ts_unittests_base", os.path.join(HERE, "run-ts-unittests-config-sys-oracle.py")
)
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)

base.GROUPS = GROUPS
base.BASELINE = os.path.join(os.path.dirname(HERE), "ts-suite", "unittests-watch.json")
base.SUITE_OF = {"tscWatch": "tscWatch-", "tsbuildWatch": "tsbuildWatch-"}

# tsbuildWatch/publicApi.ts has a top-level `it` (no describe), which the pin files under "other", not in its
# 81 tsbuildWatch tests; the group is the 82nd.
base.UNPINNED_GROUPS = ("tsbuildWatch-publicApi",)

if __name__ == "__main__":
    sys.exit(base.main())
