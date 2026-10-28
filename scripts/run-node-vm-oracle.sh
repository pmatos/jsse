#!/usr/bin/env bash
# Cross-check the embedded `vm` module (issue #1238) against Node on this host.
#
# Usage:
#   ./scripts/run-node-vm-oracle.sh [--node] [--no-cross-check]
#
# scripts/vm-fixtures/probe.js prints one line per scenario: contexts and
# contextified objects (reads, write-back, var/let/function, builtins, realms,
# accessors, read-only and frozen sandboxes), Script (options, cachedData,
# sourceURL) and compileFunction (parsing context, extensions); errors.js covers
# error behaviour (SyntaxError header with file name and line, frames of scripts
# with file name, line and column offsets). jsse's stdout must equal Node's
# byte for byte, under the bytecode VM and the tree-walker, and Node's own
# output must equal the pinned expected-<name>.txt.
#
# Options:
#   --node             run on Node only
#   --no-cross-check   run on jsse only (exit 0 required)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
FIXTURES="$PROJECT_DIR/scripts/vm-fixtures"
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
for name in probe errors; do
    PROBE="$FIXTURES/$name.js"
    PIN="$FIXTURES/expected-$name.txt"
    if [ "$CROSS_CHECK" -eq 1 ] || [ "$NODE_ONLY" -eq 1 ]; then
        rc=0
        "$NODE_BIN" "$PROBE" > "$WORK/node.out" 2> "$WORK/node.err" || rc=$?
        if [ "$rc" -ne 0 ]; then
            echo "FAIL $name: node exit $rc"
            head -5 "$WORK/node.err" | sed 's/^/  stderr: /'
            exit 1
        fi
        if ! diff -u "$PIN" "$WORK/node.out" > "$WORK/pin.txt"; then
            echo "FAIL $name: $PIN is not this Node's output (regenerate it with: node $PROBE > $PIN)"
            head -20 "$WORK/pin.txt"
            exit 1
        fi
    fi
    for flags in "" "--no-bytecode"; do
        label="$name ${flags:-bytecode}"
        if [ "$NODE_ONLY" -eq 1 ]; then
            echo "ok   $label (node only)"
            continue
        fi
        rc=0
        # shellcheck disable=SC2086
        "$JSSE" --node $flags "$PROBE" > "$WORK/jsse.out" 2> "$WORK/jsse.err" || rc=$?
        if [ "$rc" -ne 0 ]; then
            echo "FAIL $label: jsse --node exit $rc"
            head -5 "$WORK/jsse.err" | sed 's/^/  stderr: /'
            FAIL=1
        elif ! diff -u "$PIN" "$WORK/jsse.out" > "$WORK/diff.txt"; then
            echo "FAIL $label: stdout differs (--- node, +++ jsse)"
            head -40 "$WORK/diff.txt"
            FAIL=1
        else
            echo "ok   $label"
        fi
    done
done

if [ "$FAIL" -ne 0 ]; then
    echo "node-vm oracle: FAILED"
    exit 1
fi
echo "node-vm oracle: all passed"
