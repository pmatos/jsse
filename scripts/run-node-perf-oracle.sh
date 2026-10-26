#!/usr/bin/env bash
# Cross-check the Node host process metrics and the embedded `perf_hooks`
# module (issue #1227) against Node on this host.
#
# Usage:
#   ./scripts/run-node-perf-oracle.sh [--node] [--no-cross-check]
#
# scripts/perf-fixtures/probe.js prints one line per fact: process platform,
# arch, release/features/config shapes, property descriptors and function
# names, Node-style argument errors for hrtime/cpuUsage/umask, performance
# mark/measure/observer/timerify behaviour and ordering, the HdrHistogram
# numbers of createHistogram, and booleans for the values that move between
# calls (hrtime, uptime, memory, cpu, performance.now are monotonic and
# plausible). jsse's stdout must equal Node's byte for byte.
#
# Options:
#   --node             run on Node only
#   --no-cross-check   run on jsse only (exit 0 required)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PROBE="$PROJECT_DIR/scripts/perf-fixtures/probe.js"
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
for flags in "" "--no-bytecode"; do
    label="${flags:-default}"
    if [ "$NODE_ONLY" -eq 0 ]; then
        rc=0
        # shellcheck disable=SC2086
        "$JSSE" --node $flags "$PROBE" > "$WORK/jsse.out" 2> "$WORK/jsse.err" || rc=$?
        if [ "$rc" -ne 0 ]; then
            echo "FAIL $label: jsse --node exit $rc"
            head -5 "$WORK/jsse.err" | sed 's/^/  stderr: /'
            FAIL=1
            continue
        fi
    fi
    if [ "$CROSS_CHECK" -eq 1 ] || [ "$NODE_ONLY" -eq 1 ]; then
        rc=0
        "$NODE_BIN" "$PROBE" > "$WORK/node.out" 2> "$WORK/node.err" || rc=$?
        if [ "$rc" -ne 0 ]; then
            echo "FAIL $label: node exit $rc"
            head -5 "$WORK/node.err" | sed 's/^/  stderr: /'
            FAIL=1
            continue
        fi
        if [ "$NODE_ONLY" -eq 0 ] && ! diff -u "$WORK/node.out" "$WORK/jsse.out" > "$WORK/diff.txt"; then
            echo "FAIL $label: stdout differs (--- node, +++ jsse)"
            head -40 "$WORK/diff.txt"
            FAIL=1
            continue
        fi
    fi
    echo "ok   $label"
done

if [ "$FAIL" -ne 0 ]; then
    echo "node-perf oracle: FAILED"
    exit 1
fi
echo "node-perf oracle: all passed"
