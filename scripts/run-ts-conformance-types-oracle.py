#!/usr/bin/env python3
"""Run TypeScript's `conformance` runner tests of tests/cases/conformance/types on Node and jsse --node (issue #1274).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-conformance-types-oracle.py [oracle options] [slice ...]

It is scripts/run-ts-compiler-oracle.py (options, comparison, control, ratchet: docs/agents/library-tests-typescript-
compiler.md) pointed at the `conformance` runner instead of `compiler`: the oracle module is loaded and its runner
settings are replaced (mocha `test.config` runners, suite grep, the slice module scripts/ts_conformance_slices_1274.py,
the pinned count from the `types` bucket of ts-suite/oracle.json, the known failures in
ts-suite/conformance-types-known-failures.txt). Slices: the default `types` (857 suites, 5142 tests), a subdirectory
(`union`, `members`, ...), or any term of ts_conformance_slices_1274.py. --require, --jobs (at most 4) and the rest
are the oracle's.
"""

import importlib.util
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
import ts_conformance_slices_1274 as conf  # noqa: E402

KNOWN = os.path.join(ROOT, "ts-suite", "conformance-types-known-failures.txt")
# types/literal/templateLiteralTypes1.ts type-checks in about 470 s on jsse (3 s on Node) in the mocha `before` hook of
# its suite: past the compiler oracle's 600 s defaults on a loaded host, so both limits are raised for this runner.
TEST_TIMEOUT_MS = 1800000
PROCESS_TIMEOUT_S = 7200


def load_oracle():
    spec = importlib.util.spec_from_file_location(
        "run_ts_compiler_oracle", os.path.join(HERE, "run-ts-compiler-oracle.py")
    )
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def configure(oracle):
    for name in ("slices", "CONFIG", "SUITE_GREP", "KNOWN", "pinned_expected"):
        if not hasattr(oracle, name):
            sys.exit(f"run-ts-compiler-oracle.py has no {name}: update this wrapper")
    oracle.slices = conf
    oracle.CONFIG = {"runners": ["conformance"], "light": False}
    oracle.SUITE_GREP = "^conformance tests conformance tests for "
    oracle.KNOWN = KNOWN

    def pinned_expected(spec, table, runner="conformance"):
        try:
            with open(oracle.SUITES, encoding="utf-8") as f:
                buckets = json.load(f)["runners"][runner]["buckets"]
        except (OSError, ValueError, KeyError):
            return None
        return conf.pinned_directory(spec, table, buckets)

    oracle.pinned_expected = pinned_expected
    return oracle


def main(argv):
    oracle = configure(load_oracle())
    argv = list(argv)
    for flag, value in (
        ("--known", KNOWN),
        ("--test-timeout", str(TEST_TIMEOUT_MS)),
        ("--timeout", str(PROCESS_TIMEOUT_S)),
    ):
        if not any(a == flag or a.startswith(flag + "=") for a in argv):
            argv += [flag, value]
    sys.argv = [sys.argv[0]] + argv
    return oracle.main()


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
