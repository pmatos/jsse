#!/usr/bin/env bash
# Run the vendored web-platform-tests URL data (scripts/url-fixtures/wpt, issue
# #1230) on `jsse --node` and on Node and require identical output equal to
# scripts/url-fixtures/wpt/expected.output.
#
# Usage:
#   ./scripts/run-node-url-wpt.sh [--node] [--no-cross-check]
#
# Options:
#   --node             run on Node only (reference / regenerating expected.output)
#   --no-cross-check   run on jsse only

set -euo pipefail
export LC_ALL=C

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DIR="$PROJECT_DIR/scripts/url-fixtures/wpt"
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

FAIL=0
if [ "$NODE_ONLY" -eq 0 ]; then
    rc=0
    "$JSSE" --node "$DIR/run.js" > "$WORK/jsse.out" 2> "$WORK/jsse.err" || rc=$?
    if [ "$rc" -ne 0 ] || [ -s "$WORK/jsse.err" ]; then
        echo "FAIL wpt: jsse exit $rc"
        head -5 "$WORK/jsse.err" | sed 's/^/  stderr: /'
        FAIL=1
    elif ! diff -u "$DIR/expected.output" "$WORK/jsse.out" > "$WORK/diff.txt"; then
        echo "FAIL wpt: jsse output differs from expected.output (--- expected, +++ jsse)"
        head -60 "$WORK/diff.txt"
        FAIL=1
    else
        echo "ok   wpt: jsse matches expected.output"
    fi
fi
if [ "$CROSS_CHECK" -eq 1 ] || [ "$NODE_ONLY" -eq 1 ]; then
    rc=0
    "$NODE_BIN" "$DIR/run.js" > "$WORK/node.out" 2> "$WORK/node.err" || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "FAIL wpt: node exit $rc"
        FAIL=1
    elif ! diff -u "$DIR/expected.output" "$WORK/node.out" > "$WORK/diff.txt"; then
        echo "FAIL wpt: node output differs from expected.output (--- expected, +++ node)"
        head -60 "$WORK/diff.txt"
        FAIL=1
    else
        echo "ok   wpt: node matches expected.output"
    fi
fi

if [ "$FAIL" -ne 0 ]; then
    echo "node-url wpt: FAILED"
    exit 1
fi
echo "node-url wpt: all passed"
