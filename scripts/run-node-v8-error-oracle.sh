#!/usr/bin/env bash
# Cross-check the V8-compatible Error API (issue #1228) against Node.
#
# Usage:
#   ./scripts/run-node-v8-error-oracle.sh [--node] [--no-cross-check]
#
# scripts/v8-error-fixtures/cases.js runs on Node and on `jsse --node`. Each
# run's stdout is normalised (the fixture directory becomes <dir>; stack frames
# outside cases.js, and the top-level `Object.<anonymous>` frame Node adds, are
# dropped) and the two must be identical and equal expected-cases.txt, which
# is Node's own output.
#
# Options:
#   --node             run on Node only and rewrite expected-cases.txt
#   --no-cross-check   run on jsse only and compare with expected-cases.txt

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DIR="$SCRIPT_DIR/v8-error-fixtures"
JSSE="${JSSE:-$PROJECT_DIR/target/release/jsse}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

NODE_ONLY=0
CROSS_CHECK=1
for arg in "$@"; do
    case "$arg" in
        --node) NODE_ONLY=1 ;;
        --no-cross-check) CROSS_CHECK=0 ;;
        *) echo "unknown option: $arg" >&2; exit 2 ;;
    esac
done

NODE_BIN="$(command -v node || true)"
if [ -z "$NODE_BIN" ] && { [ "$CROSS_CHECK" -eq 1 ] || [ "$NODE_ONLY" -eq 1 ]; }; then
    echo "WARNING: node not found; running jsse only" >&2
    CROSS_CHECK=0
    NODE_ONLY=0
fi
if [ "$NODE_ONLY" -eq 0 ] && [ ! -x "$JSSE" ]; then
    echo "Building jsse (release)..."
    (cd "$PROJECT_DIR" && cargo build --release)
fi

normalise() {
    sed -e "s#$DIR#<dir>#g" | awk '
        /^[[:space:]]+at / { if ($0 ~ /<dir>\/cases\.js/ && $0 !~ /Object\.<anonymous>/) print; next }
        { print }'
}

FAIL=0
if [ "$NODE_ONLY" -eq 1 ]; then
    "$NODE_BIN" "$DIR/cases.js" 2>&1 | normalise > "$DIR/expected-cases.txt"
    echo "wrote $DIR/expected-cases.txt"
    exit 0
fi

"$JSSE" --node "$DIR/cases.js" 2>&1 | normalise > "$WORK/jsse.out" || true
if ! diff -u "$DIR/expected-cases.txt" "$WORK/jsse.out" > "$WORK/jsse.diff"; then
    echo "FAIL: jsse differs from expected-cases.txt (--- expected, +++ jsse)"
    head -80 "$WORK/jsse.diff"
    FAIL=1
else
    echo "ok   jsse matches expected-cases.txt"
fi
if [ "$CROSS_CHECK" -eq 1 ]; then
    "$NODE_BIN" "$DIR/cases.js" 2>&1 | normalise > "$WORK/node.out" || true
    if ! diff -u "$DIR/expected-cases.txt" "$WORK/node.out" > "$WORK/node.diff"; then
        echo "FAIL: node differs from expected-cases.txt (stale snapshot?)"
        head -40 "$WORK/node.diff"
        FAIL=1
    else
        echo "ok   node matches expected-cases.txt"
    fi
fi
if [ "$FAIL" -ne 0 ]; then
    echo "node-v8-error oracle: FAILED"
    exit 1
fi
echo "node-v8-error oracle: all passed"
