#!/usr/bin/env bash
# Cross-check MessageChannel/MessagePort and the structuredClone transfer
# protocol of `jsse --node` (issue #1528) against Node.
#
# Usage:
#   ./scripts/run-node-messaging-oracle.sh [--node] [--no-cross-check] [--update]
#
# Each scripts/messaging-fixtures/*.fixture.js must exit 0 and print the
# <name>.stdout beside it, on jsse (`--node`, bytecode and tree-walker) and on
# Node. The .stdout files are Node's own output.
#
# Options:
#   --node             run on Node only (reference / debugging)
#   --no-cross-check   run on jsse only
#   --update           rewrite the <name>.stdout files from Node's output

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DIR="$SCRIPT_DIR/messaging-fixtures"
JSSE="${JSSE:-$PROJECT_DIR/target/release/jsse}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

NODE_ONLY=0
CROSS_CHECK=1
UPDATE=0
for arg in "$@"; do
    case "$arg" in
        --node) NODE_ONLY=1 ;;
        --no-cross-check) CROSS_CHECK=0 ;;
        --update) UPDATE=1; NODE_ONLY=1 ;;
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

# <name> <label> <command...>: stdout must equal the golden file, exit 0.
check() {
    local name="$1" label="$2"
    shift 2
    local rc=0
    (cd "$DIR" && exec timeout 120 "$@" "$name.fixture.js") > "$WORK/out" 2> "$WORK/err" || rc=$?
    if [ "$UPDATE" -eq 1 ]; then
        cp "$WORK/out" "$DIR/$name.stdout"
        echo "wrote $name.stdout"
    elif [ "$rc" -ne 0 ]; then
        echo "FAIL $name ($label): exit $rc"
        head -5 "$WORK/err" | sed 's/^/  stderr: /'
        FAIL=1
    elif ! diff -u "$DIR/$name.stdout" "$WORK/out" > "$WORK/diff"; then
        echo "FAIL $name ($label): stdout differs (--- expected, +++ $label)"
        cat "$WORK/diff"
        FAIL=1
    else
        echo "ok   $name ($label)"
    fi
}

FAIL=0
shopt -s nullglob
for fixture in "$DIR"/*.fixture.js; do
    name="$(basename "${fixture%.fixture.js}")"
    if [ "$NODE_ONLY" -eq 1 ] || [ "$CROSS_CHECK" -eq 1 ]; then
        check "$name" node "$NODE_BIN"
    fi
    if [ "$NODE_ONLY" -eq 0 ]; then
        check "$name" jsse "$JSSE" --node
        check "$name" jsse-tree-walker "$JSSE" --node --no-bytecode
    fi
done

if [ "$FAIL" -ne 0 ]; then
    echo "node-messaging oracle: FAILED"
    exit 1
fi
echo "node-messaging oracle: all passed"
