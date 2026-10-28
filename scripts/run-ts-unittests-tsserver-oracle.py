#!/usr/bin/env python3
"""Run TypeScript's tsserver unit tests (unittests/tsserver/*.ts) on Node and on jsse --node and diff them per test (#1266, #1267).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-unittests-tsserver-oracle.py [--half a-m|n-z|all] [options] [group ...]

The machinery is scripts/run-ts-unittests-config-sys-oracle.py (same options, same per-title comparison, same
checkout requirements, --record, --list) with the groups of scripts/ts_unittest_tsserver_groups.py, the baseline
ts-suite/unittests-tsserver.json and the pin of the `tsserver` suite in ts-suite/oracle.json (1240 tests).

--half picks what a run without group arguments executes: the files of unittests/tsserver whose name starts with
a-m (default, #1266), n-z (#1267) or all of them. The table lists every file of both halves; before a run, with
TS_SRC_DIR set, it is checked against the checkout (a file or a top-level describe no group selects fails the run).
The pinned total covers both halves, so it is checked against the baseline counts of the groups this run left out.
"""

import argparse
import importlib.util
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import ts_unittest_tsserver_groups as table  # noqa: E402

spec = importlib.util.spec_from_file_location(
    "run_ts_unittests_base", os.path.join(HERE, "run-ts-unittests-config-sys-oracle.py")
)
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)

base.GROUPS = table.GROUPS
base.BASELINE = os.path.join(
    os.path.dirname(HERE), "ts-suite", "unittests-tsserver.json"
)
base.SUITE_OF = {"tsserver": "tsserver-"}
base.TEST_TIMEOUT_MS = 600000

_pinned = base.pinned_suite_problems


def pinned_with_baseline(new_counts):
    """Counts of the groups this run did not execute come from the baseline."""
    try:
        with open(base.BASELINE, encoding="utf-8") as f:
            recorded = json.load(f).get("counts", {})
    except (OSError, ValueError):
        recorded = {}
    return _pinned({**recorded, **new_counts})


base.pinned_suite_problems = pinned_with_baseline


def main(argv):
    ap = argparse.ArgumentParser(add_help=False)
    ap.add_argument("--half", default="a-m", choices=(*table.HALVES, "all"))
    o, rest = ap.parse_known_args(argv)
    half = o.half
    base.DEFAULT_GROUPS = table.groups_of_half(half)
    given = os.environ.get("TS_SRC_DIR")
    ts_dir = os.path.realpath(given) if given else None
    if ts_dir and "--list" not in rest and base.check_checkout(ts_dir) is None:
        problems = table.check_groups(ts_dir)
        if problems:
            print("tsserver group table does not match the checkout:")
            print("\n".join("  " + p for p in problems))
            return 1
    sys.argv[1:] = rest
    return base.main()


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
