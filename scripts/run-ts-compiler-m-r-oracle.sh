#!/usr/bin/env bash
# Compiler runner, test files M-R (issue #1272): Node vs `jsse --node`, per test, with the known failures
# of ts-suite/compiler-known-failures-m-r.txt. Extra arguments go to run-ts-compiler-oracle.py (--verbose,
# --record, --batch-size N, --jsse PATH, ...; options only, the slice is always issue1272); needs TS_SRC_DIR (a built TypeScript v6.0.3 checkout), else SKIP.
set -euo pipefail
root="$(dirname "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")")"
exec uv run python "$root/scripts/run-ts-compiler-oracle.py" \
    --slices-module ts_compiler_slices_1272 \
    --known "$root/ts-suite/compiler-known-failures-m-r.txt" \
    "$@" issue1272
