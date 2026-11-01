#!/usr/bin/env bash
# Cross-check V8 message text of builtin errors and engine-thrown error
# positions (issues #1567, #1568, #1557) against Node.
#
# Usage:
#   ./scripts/run-node-v8-messages-1567-oracle.sh [--node] [--no-cross-check] [--no-bytecode]
#
# Every *.js in tests/node-v8-messages-1567 is run on Node and on `jsse --node`
# (an `eval-*.js` as `-e`), stdout and stderr together. The output is
# normalised (the fixture directory becomes <dir>, `node:` frames and the
# version line are dropped) and the two must be identical and equal the
# fixture's .out file, which is Node's own output.
#
# Options:
#   --node             run on Node only and rewrite the expected files
#   --no-cross-check   run on jsse only and compare with the expected files
#   --no-bytecode      run jsse on the tree-walker

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DIR="$PROJECT_DIR/tests/node-v8-messages-1567"
JSSE="${JSSE:-$PROJECT_DIR/target/release/jsse}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

NODE_ONLY=0
CROSS_CHECK=1
JSSE_ARGS=(--node)
for arg in "$@"; do
    case "$arg" in
        --node) NODE_ONLY=1 ;;
        --no-cross-check) CROSS_CHECK=0 ;;
        --no-bytecode) JSSE_ARGS+=(--no-bytecode) ;;
        *) echo "unknown option: $arg" >&2; exit 2 ;;
    esac
done

NODE_BIN="$(command -v node || true)"
if [ -z "$NODE_BIN" ]; then
    echo "WARNING: node not found; running jsse only" >&2
    CROSS_CHECK=0
    NODE_ONLY=0
fi
if [ "$NODE_ONLY" -eq 0 ] && [ ! -x "$JSSE" ]; then
    echo "Building jsse (release)..."
    (cd "$PROJECT_DIR" && cargo build --release)
fi

normalise() {
    sed -e "s#$DIR#<dir>#g" -e 's#^Node\.js v.*#Node.js <version>#' |
        grep -v -E '^[[:space:]]+at .*node:'
}

run() {
    local engine=$1 fixture=$2 mode=$3
    shift 3
    (cd "$DIR" && if [ "$mode" = eval ]; then
        "$engine" "$@" -e "$(cat "$fixture")"
    else
        "$engine" "$@" "$fixture"
    fi) 2>&1 | normalise
}

FAIL=0
for FIXTURE in "$DIR"/*.js; do
    NAME="$(basename "$FIXTURE" .js)"
    MODE=file
    case "$NAME" in eval-*) MODE=eval ;; esac
    EXPECTED="$DIR/$NAME.out"
    if [ "$NODE_ONLY" -eq 1 ]; then
        run "$NODE_BIN" "$FIXTURE" "$MODE" > "$EXPECTED" || true
        echo "wrote $EXPECTED"
        continue
    fi
    run "$JSSE" "$FIXTURE" "$MODE" "${JSSE_ARGS[@]}" > "$WORK/jsse.out" || true
    if ! diff -u "$EXPECTED" "$WORK/jsse.out" > "$WORK/jsse.diff"; then
        echo "FAIL: $NAME: jsse differs from the expected output (--- expected, +++ jsse)"
        head -80 "$WORK/jsse.diff"
        FAIL=1
    else
        echo "ok   $NAME: jsse matches the expected output"
    fi
    if [ "$CROSS_CHECK" -eq 1 ]; then
        run "$NODE_BIN" "$FIXTURE" "$MODE" > "$WORK/node.out" || true
        if ! diff -u "$EXPECTED" "$WORK/node.out" > "$WORK/node.diff"; then
            echo "FAIL: $NAME: node differs from the expected output (stale snapshot?)"
            head -40 "$WORK/node.diff"
            FAIL=1
        else
            echo "ok   $NAME: node matches the expected output"
        fi
    fi
done
if [ "$NODE_ONLY" -eq 1 ]; then
    exit 0
fi
if [ "$FAIL" -ne 0 ]; then
    echo "node-v8-messages-1567 oracle: FAILED"
    exit 1
fi
echo "node-v8-messages-1567 oracle: all passed"
