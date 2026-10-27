#!/usr/bin/env bash
# Cross-check the Node event-loop semantics of `jsse --node` (issue #1229)
# against Node: nextTick / microtask / timer / immediate ordering, ref and
# unref, beforeExit / exit, uncaught exceptions and unhandled rejections.
#
# Usage:
#   ./scripts/run-node-event-loop-oracle.sh [--node] [--no-cross-check]
#
# Each scripts/event-loop-fixtures/*.fixture.js has header lines:
#   // RUN: <expected-exit-status>
#   // ERR: <text>      (optional; stderr must contain it on both engines)
# A fixture passes iff stdout and the exit status match Node's and the status
# equals the RUN line. Stderr is checked for the ERR text and, when a
# <name>.stderr file sits beside the fixture, against it in full after
# normalising: the fixture directory becomes <dir>, the version line
# "Node.js <version>", every "    at ... node:..." frame (Node's internals,
# jsse's embedded modules) is dropped and the N of "... N lines matching cause
# stack trace ..." is blanked. Node's own stderr must normalise to the same
# file, so the expectations cannot drift from Node.
# Then the opt-in gate is checked: without --node (and with --prelude, which
# test262 uses) the event-loop globals and process events must not exist, and
# timers keep returning numbers.
#
# Options:
#   --node             run on Node only (reference / debugging)
#   --no-cross-check   run on jsse only, compare against the RUN line alone
#   --update-stderr    rewrite the <name>.stderr files from Node's stderr

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
FIXTURE_DIR="$(cd "$SCRIPT_DIR/event-loop-fixtures" && pwd -P)"
JSSE="${JSSE:-$PROJECT_DIR/target/release/jsse}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

NODE_ONLY=0
CROSS_CHECK=1
UPDATE_STDERR=0
for arg in "$@"; do
    case "$arg" in
        --node) NODE_ONLY=1 ;;
        --no-cross-check) CROSS_CHECK=0 ;;
        --update-stderr) UPDATE_STDERR=1; NODE_ONLY=1 ;;
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

# stderr with the machine-specific parts replaced (see the header comment)
normalise_stderr() {
    local text
    text="$(cat "$1")"
    printf '%s\n' "${text//"$FIXTURE_DIR"/<dir>}" \
        | sed -E -e '/^[[:space:]]+at (async )?(.* \()?node:/d' \
            -e 's/^Node\.js v[0-9][0-9.]*$/Node.js <version>/' \
            -e 's/\.\.\. [0-9]+ lines matching cause/... N lines matching cause/'
}

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
    err_text="$(sed -n 's|^// ERR: *||p' "$fixture" | head -1)"
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
        elif [ -n "$err_text" ] && ! grep -qF -- "$err_text" "$WORK/$engine.err"; then
            echo "FAIL $name: $engine stderr lacks '$err_text'"
            sed 's/^/  stderr: /' "$WORK/$engine.err" | head -5
            ok=0
        elif [ "$UPDATE_STDERR" -eq 1 ]; then
            rm -f "${fixture%.fixture.js}.stderr"
            if grep -q '^Node\.js v' "$WORK/$engine.err"; then
                normalise_stderr "$WORK/$engine.err" > "${fixture%.fixture.js}.stderr"
            fi
        elif [ -f "${fixture%.fixture.js}.stderr" ]; then
            normalise_stderr "$WORK/$engine.err" > "$WORK/$engine.norm"
            if ! diff -u "${fixture%.fixture.js}.stderr" "$WORK/$engine.norm" > "$WORK/diff.txt"; then
                echo "FAIL $name: $engine stderr differs (--- expected, +++ $engine)"
                cat "$WORK/diff.txt"
                ok=0
            fi
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
    GATE='var r = [typeof setImmediate, typeof clearImmediate, typeof queueMicrotask, typeof process, typeof setTimeout(function () {}, 0), typeof __host_loop_install, typeof Symbol.for("jsse.node.builtins") === "symbol" && typeof globalThis[Symbol.for("jsse.node.builtins")]]; print(r.join(" "))'
    EXPECT='undefined undefined undefined undefined number undefined undefined'
    echo 'void 0;' > "$WORK/empty-prelude.js"
    for mode in plain prelude; do
        args=()
        [ "$mode" = prelude ] && args=(--prelude "$WORK/empty-prelude.js")
        got="$("$JSSE" "${args[@]}" -e "$GATE" 2>&1 || true)"
        if [ "$got" != "$EXPECT" ]; then
            echo "FAIL gate ($mode): event-loop surface visible without --node: $got"
            FAIL=1
        else
            echo "ok   gate ($mode): event-loop surface absent without --node"
        fi
    done
fi

if [ "$FAIL" -ne 0 ]; then
    echo "node-event-loop oracle: FAILED"
    exit 1
fi
echo "node-event-loop oracle: all passed"
