#!/usr/bin/env bash
# Run code quality checks: clippy and rustfmt.
#
# Usage:
#   ./scripts/lint.sh [--fix]
#
# With --fix, applies automatic fixes where possible.

set -euo pipefail

FIX=false
if [[ "${1:-}" == "--fix" ]]; then
    FIX=true
fi

echo "=== rustfmt ==="
if $FIX; then
    cargo fmt
    echo "Formatted."
else
    cargo fmt --check
    echo "Format OK."
fi

echo ""
echo "=== clippy ==="
if $FIX; then
    cargo clippy --fix --allow-dirty --allow-staged -- -D warnings
    echo "Fixed."
else
    cargo clippy -- -D warnings
    echo "Clippy OK."
fi

echo ""
echo "All checks passed."
