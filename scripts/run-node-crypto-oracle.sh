#!/usr/bin/env bash
# Cross-check the embedded `crypto` module (issue #1226) against Node.
#
# Usage:
#   ./scripts/run-node-crypto-oracle.sh [--node] [--no-cross-check]
#
# scripts/crypto-fixtures/{hashes,random}.js print one line per check: digests of every
# supported algorithm over known strings, block-boundary lengths (every two-chunk
# split and byte-wise streaming), 1 MiB / 16 MiB generated inputs (streamed
# through odd chunk sizes and offset views), copy(), every input and output
# encoding, HMAC keys around the block size, hash(), timingSafeEqual, the error
# codes and messages, the shape of the random values, and subtle.digest. jsse's
# stdout must equal Node's byte for byte (warnings go to stderr and are not
# compared).
#
# Options:
#   --node             run on Node only
#   --no-cross-check   run on jsse only (exit 0 required)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
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
LINES=0
for fixture in hashes random; do
    FILE="$PROJECT_DIR/scripts/crypto-fixtures/$fixture.js"
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
                head -40 "$WORK/diff.txt"
                status="FAIL"; FAIL=1
            fi
        fi
    fi
    echo "$status $fixture"
done

if [ "$FAIL" -ne 0 ]; then
    echo "node-crypto oracle: FAILED"
    exit 1
fi
echo "node-crypto oracle: all passed"
