#!/usr/bin/env bash
# Cross-check `child_process` and process signals of `jsse --node` (issue
# #1236) against Node: spawn/exec/execFile and their sync forms with stdio
# pipes, inherit and ignore, cwd, env, shell, timeout, killSignal, abort
# signals, exit codes and signals, ENOENT, large outputs and inputs (checked
# by length and MD5), jsse itself as the child, process.kill and
# process.on('SIGxxx').
#
# Usage:
#   ./scripts/run-node-child-process-oracle.sh [--node] [--no-cross-check]
#
# Each scripts/child-process-fixtures/*.fixture.js has a header line
#   // RUN: <expected-exit-status>
# and prints only values Node makes deterministic (events are collected up to
# 'close'). A fixture passes iff stdout and the exit status match Node's and the
# status equals the RUN line. A "// NEEDS: <module>" line skips the fixture
# while jsse cannot require that module. Then the opt-in gate is checked: without --node
# (and with --prelude, which test262 uses) none of the floor exists.
#
# Options:
#   --node             run on Node only (reference / debugging)
#   --no-cross-check   run on jsse only, compare against the RUN line alone

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
FIXTURE_DIR="$SCRIPT_DIR/child-process-fixtures"
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
if [ -z "$NODE_BIN" ] && [ "$CROSS_CHECK" -eq 1 ]; then
    echo "WARNING: node not found; running jsse only" >&2
    CROSS_CHECK=0
fi
if [ "$NODE_ONLY" -eq 0 ] && [ ! -x "$JSSE" ]; then
    echo "Building jsse (release)..."
    (cd "$PROJECT_DIR" && cargo build --release)
fi

FAIL=0
shopt -s nullglob
FIXTURES=("$FIXTURE_DIR"/*.fixture.js)
if [ "${#FIXTURES[@]}" -eq 0 ]; then
    echo "No fixtures found in $FIXTURE_DIR" >&2
    exit 2
fi

# <out-prefix> <engine...>; writes .out/.err, sets RC (a timeout is a failure)
run_engine() {
    local prefix="$1"
    shift
    RC=0
    timeout 60 "$@" > "$prefix.out" 2> "$prefix.err" || RC=$?
}

for fixture in "${FIXTURES[@]}"; do
    name="$(basename "$fixture" .fixture.js)"
    expect="$(sed -n 's|^// RUN: *||p' "$fixture" | head -1)"
    needs="$(sed -n 's|^// NEEDS: *||p' "$fixture" | head -1)"
    if [ -n "$needs" ] && [ "$NODE_ONLY" -eq 0 ] \
        && ! "$JSSE" --node -e "require('$needs')" > /dev/null 2>&1; then
        echo "skip $name (needs the $needs module)"
        continue
    fi
    engines=()
    [ "$NODE_ONLY" -eq 0 ] && engines+=(jsse)
    { [ "$CROSS_CHECK" -eq 1 ] || [ "$NODE_ONLY" -eq 1 ]; } && engines+=(node)
    ok=1
    for engine in "${engines[@]}"; do
        if [ "$engine" = jsse ]; then
            run_engine "$WORK/$engine" "$JSSE" --node "$fixture"
        else
            run_engine "$WORK/$engine" "$NODE_BIN" "$fixture"
        fi
        if [ "$RC" -ne "$expect" ]; then
            echo "FAIL $name: $engine exit $RC, expected $expect"
            sed 's/^/  stderr: /' "$WORK/$engine.err" | head -5
            ok=0
        fi
    done
    if [ "$ok" -eq 1 ] && [ "${#engines[@]}" -eq 2 ] \
        && ! diff -u "$WORK/node.out" "$WORK/jsse.out" > "$WORK/diff.txt"; then
        echo "FAIL $name: stdout differs (--- node, +++ jsse)"
        cat "$WORK/diff.txt"
        ok=0
    fi
    if [ "$ok" -eq 1 ]; then
        echo "ok   $name (exit $expect)"
    else
        FAIL=1
    fi
done

if [ "$NODE_ONLY" -eq 0 ]; then
    GATE='var r = [typeof process, typeof __host_cp_spawn, typeof __host_cp_spawn_sync, typeof __host_kill, typeof __host_signal_watch, typeof require, typeof Symbol.for("jsse.node.child") === "symbol" && typeof globalThis[Symbol.for("jsse.node.child")]]; print(r.join(" "))'
    EXPECT='undefined undefined undefined undefined undefined undefined undefined'
    echo 'void 0;' > "$WORK/empty-prelude.js"
    for mode in plain prelude; do
        args=()
        [ "$mode" = prelude ] && args=(--prelude "$WORK/empty-prelude.js")
        got="$("$JSSE" "${args[@]}" -e "$GATE" 2>&1 || true)"
        if [ "$got" != "$EXPECT" ]; then
            echo "FAIL gate ($mode): child_process surface visible without --node: $got"
            FAIL=1
        else
            echo "ok   gate ($mode): child_process surface absent without --node"
        fi
    done
fi

if [ "$FAIL" -ne 0 ]; then
    echo "node-child-process oracle: FAILED"
    exit 1
fi
echo "node-child-process oracle: all passed"
