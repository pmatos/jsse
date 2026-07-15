#!/usr/bin/env bash
# Run acorn's test suite on jsse.
#
# Thin wrapper kept for backwards compatibility: acorn is now one library
# config on the generalized harness (scripts/libs/acorn.sh). See
# scripts/README.md for the full recipe.
#
# Usage:
#   ./scripts/run-acorn-tests.sh [--clean] [--node]
#
# Flags are forwarded verbatim to run-library-tests.sh.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
exec "$SCRIPT_DIR/run-library-tests.sh" acorn "$@"
