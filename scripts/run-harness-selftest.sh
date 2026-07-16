#!/usr/bin/env bash
# Self-test for the in-process test-runner harness (scripts/node-test-harness.js).
#
# Unlike run-shim-fixtures.sh (which cross-checks the Buffer/TextEncoder shim
# against Node's natives), the harness is jsse-only by design: it is inert on
# real Node, where a suite's own framework runs instead. So these fixtures are
# validated on jsse alone. Each fixture under scripts/harness-fixtures/ drives
# the QUnit adapter or the describe/it TAP runner through a deterministic mix of
# passing and failing tests and declares the exact summary line it must produce:
#
#     // Expected summary: PASS: <p>  FAIL: <f>  TOTAL: <t>
#
# This runner prepends the shared shims exactly as run-library-tests.sh does,
# runs the fixture on jsse --node, and checks the emitted summary matches.
#
# Usage: ./scripts/run-harness-selftest.sh [--no-build]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
JSSE="$PROJECT_DIR/target/release/jsse"
FIXTURE_DIR="$SCRIPT_DIR/harness-fixtures"

NO_BUILD=0
for arg in "$@"; do
    case "$arg" in
        --no-build) NO_BUILD=1 ;;
        *) echo "unknown option: $arg" >&2; exit 2 ;;
    esac
done

if [ "$NO_BUILD" -eq 0 ]; then
    echo "Building jsse (release)..."
    (cd "$PROJECT_DIR" && cargo build --release)
fi

SHIMS=("$SCRIPT_DIR/node-shim.js" "$SCRIPT_DIR/node-buffer-shim.js" "$SCRIPT_DIR/node-test-harness.js")
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

FAIL=0
for fixture in "$FIXTURE_DIR"/*.fixture.js; do
    [ -e "$fixture" ] || continue
    name="$(basename "$fixture")"
    expected="$(grep -oE 'Expected summary: PASS: [0-9]+  FAIL: [0-9]+  TOTAL: [0-9]+' "$fixture" | head -1 | sed 's/^Expected summary: //')"
    if [ -z "$expected" ]; then
        echo "SKIP $name (no 'Expected summary:' marker)"
        continue
    fi

    final="$TMP/$name"
    cat "${SHIMS[@]}" "$fixture" > "$final"
    out="$TMP/$name.out"
    rc=0
    "$JSSE" --node "$final" > "$out" 2>&1 || rc=$?

    actual="$(grep -oE 'PASS: [0-9]+  FAIL: [0-9]+  TOTAL: [0-9]+' "$out" | tail -1 || true)"
    if [ "$rc" -ne 0 ]; then
        echo "FAIL $name (exit $rc)"
        cat "$out"
        FAIL=1
    elif [ "$actual" != "$expected" ]; then
        echo "FAIL $name: got '$actual', expected '$expected'"
        cat "$out"
        FAIL=1
    else
        echo "PASS $name ($actual)"
    fi
done

if [ "$FAIL" -eq 0 ]; then
    echo "OK: harness self-test green"
    exit 0
fi
echo "FAILED: harness self-test" >&2
exit 1
