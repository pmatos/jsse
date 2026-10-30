#!/usr/bin/env bash
# Compiler runner, test files 0-9 and A-C (issue #1269): Node vs `jsse --node`, per test, with the known failures
# of ts-suite/compiler-known-failures-0-9-a-c.txt. Extra arguments go to run-ts-compiler-oracle.py (--verbose,
# --record, --batch-size N, --jsse PATH, ...; options only, the slice is always issue1269); needs TS_SRC_DIR (a built TypeScript v6.0.3 checkout), else SKIP.
set -euo pipefail
root="$(dirname "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")")"
exec uv run python "$root/scripts/run-ts-compiler-oracle.py" \
    --slices-module ts_compiler_slices_1269 \
    --known "$root/ts-suite/compiler-known-failures-0-9-a-c.txt" \
    "$@" issue1269
