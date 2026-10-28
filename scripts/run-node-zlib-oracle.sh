#!/usr/bin/env bash
# Cross-check the embedded `zlib` module (issue #1239) against Node.
#
# Usage:
#   ./scripts/run-node-zlib-oracle.sh [--node] [--no-cross-check]
#
# scripts/zlib-fixtures/{matrix,errors,misc,streams,brotli,brotli-streams}.js
# print one line per check
# and jsse's stdout must equal Node's byte for byte: the sync/callback round
# trips over every mode, level, strategy, windowBits and memLevel, compressed
# bytes where the algorithm pins them (stored, Huffman-only, RLE, tiny inputs),
# error messages/codes/errnos, option validation, constants, crc32, Node-made
# golden streams, and stream chunk boundaries and event order. The Brotli
# fixtures do the same for every quality, window, mode, size hint, dictionary,
# error code and stream operation; their compressed bytes are compared at the
# qualities where the `brotli` crate matches libbrotli. cross.js then
# makes each engine decode the streams the other compressed (levels where Node's
# Chromium zlib and jsse's zlib-rs legitimately differ are compared only by
# decoding, so every line must read "ok").
#
# Options:
#   --node             run on Node only
#   --no-cross-check   run on jsse only (exit 0 required)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
JSSE="${JSSE:-$PROJECT_DIR/target/release/jsse}"
FIXTURES="$PROJECT_DIR/scripts/zlib-fixtures"
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
for fixture in matrix errors misc streams brotli brotli-streams; do
    FILE="$FIXTURES/$fixture.js"
    status="ok  "
    if [ "$NODE_ONLY" -eq 0 ]; then
        rc=0
        "$JSSE" --node "$FILE" > "$WORK/jsse.out" 2> "$WORK/jsse.err" || rc=$?
        if [ "$rc" -ne 0 ]; then
            echo "FAIL $fixture: jsse --node exit $rc"
            head -5 "$WORK/jsse.err" | sed 's/^/  stderr: /'
            status="FAIL"; FAIL=1
        fi
    fi
    if [ "$CROSS_CHECK" -eq 1 ] || [ "$NODE_ONLY" -eq 1 ]; then
        rc=0
        "$NODE_BIN" "$FILE" > "$WORK/node.out" 2> "$WORK/node.err" || rc=$?
        if [ "$rc" -ne 0 ]; then
            echo "FAIL $fixture: node exit $rc"
            head -5 "$WORK/node.err" | sed 's/^/  stderr: /'
            status="FAIL"; FAIL=1
        elif [ "$NODE_ONLY" -eq 0 ] && [ "$status" != "FAIL" ]; then
            if ! diff -u "$WORK/node.out" "$WORK/jsse.out" > "$WORK/diff.txt"; then
                echo "FAIL $fixture: stdout differs (--- node, +++ jsse)"
                head -40 "$WORK/diff.txt" | cut -c1-300
                status="FAIL"; FAIL=1
            fi
        fi
    fi
    echo "$status $fixture"
done

cross() {
    local engine_name="$1" produced="$2"; shift 2
    local rc=0
    "$@" "$FIXTURES/cross.js" consume "$produced" > "$WORK/consume.out" 2> "$WORK/consume.err" || rc=$?
    if [ "$rc" -ne 0 ] || grep -qv ' ok$' "$WORK/consume.out" || [ ! -s "$WORK/consume.out" ]; then
        echo "FAIL cross: $engine_name could not decode"
        grep -v ' ok$' "$WORK/consume.out" | head -10
        head -5 "$WORK/consume.err" | sed 's/^/  stderr: /'
        FAIL=1
    else
        echo "ok   cross: $engine_name decoded $(wc -l < "$WORK/consume.out") streams"
    fi
}

if [ "$NODE_ONLY" -eq 0 ]; then
    "$JSSE" --node "$FIXTURES/cross.js" produce "$WORK/jsse.json"
    cross "jsse (own streams)" "$WORK/jsse.json" "$JSSE" --node
    if [ "$CROSS_CHECK" -eq 1 ]; then
        "$NODE_BIN" "$FIXTURES/cross.js" produce "$WORK/node.json"
        cross "jsse (node streams)" "$WORK/node.json" "$JSSE" --node
        cross "node (jsse streams)" "$WORK/jsse.json" "$NODE_BIN"
    fi
fi

if [ "$FAIL" -ne 0 ]; then
    echo "node-zlib oracle: FAILED"
    exit 1
fi
echo "node-zlib oracle: all passed"
