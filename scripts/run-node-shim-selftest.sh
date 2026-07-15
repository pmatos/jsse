#!/usr/bin/env bash
# Cross-check the Node host-compat readable-output layer (scripts/node-shim.js,
# issue #230) on jsse against Node.
#
# It concatenates the shim in front of scripts/node-shim.selftest.js (exactly as
# run-library-tests.sh prepends the shim to a bundle), runs the result on jsse
# with --node and on Node, and requires BOTH to exit 0 and produce byte-identical
# stdout. Node is the reference oracle for the deterministic surfaces the
# self-test asserts (util.format specifiers, byte-accurate stdout, console
# shapes); util.inspect is only smoke-tested, never byte-compared.
#
# Usage: ./scripts/run-node-shim-selftest.sh [--no-build]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
JSSE="$PROJECT_DIR/target/release/jsse"

BUILD=1
for arg in "$@"; do
    case "$arg" in
        --no-build) BUILD=0 ;;
        *) echo "unknown option: $arg" >&2; exit 2 ;;
    esac
done

if [ "$BUILD" -eq 1 ]; then
    echo "Building jsse (release)..."
    (cd "$PROJECT_DIR" && cargo build --release)
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
FINAL="$TMP/selftest.bundle.js"
cat "$SCRIPT_DIR/node-shim.js" "$SCRIPT_DIR/node-shim.selftest.js" > "$FINAL"

run_engine() {  # <label> <cmd...> → writes $TMP/<label>.out, sets RC
    local label="$1"; shift
    RC=0
    "$@" "$FINAL" > "$TMP/$label.out" 2> "$TMP/$label.err" || RC=$?
}

FAIL=0

echo ""
echo "== jsse --node =="
run_engine jsse "$JSSE" --node
JSSE_RC="$RC"
cat "$TMP/jsse.out"
[ -s "$TMP/jsse.err" ] && { echo "--- jsse stderr ---"; cat "$TMP/jsse.err"; }
if [ "$JSSE_RC" -ne 0 ]; then
    echo "FAIL: jsse exited $JSSE_RC" >&2
    FAIL=1
fi

if command -v node >/dev/null 2>&1; then
    echo ""
    echo "== node (reference) =="
    run_engine node node
    NODE_RC="$RC"
    [ -s "$TMP/node.err" ] && { echo "--- node stderr ---"; cat "$TMP/node.err"; }
    if [ "$NODE_RC" -ne 0 ]; then
        echo "FAIL: node exited $NODE_RC" >&2
        FAIL=1
    fi

    if ! diff -u "$TMP/node.out" "$TMP/jsse.out" > "$TMP/diff.txt"; then
        echo "FAIL: jsse and Node stdout differ (--- node, +++ jsse):" >&2
        cat "$TMP/diff.txt" >&2
        FAIL=1
    fi
else
    echo "WARNING: node not found — skipping cross-check (jsse-only)" >&2
fi

# ---- process.exit probe ---------------------------------------------------
# The self-test above never drives exit (it must reach its plan line), so cover
# process.exit(code) separately: a real, uncatchable exit with the right code.
# A try/catch around it must not swallow the exit, and the trailing log must not
# run. Expected on both engines: exit code 3, no "UNREACHABLE" on stdout.
echo ""
echo "== process.exit probe =="
EXIT_PROBE="$TMP/exit-probe.js"
cat "$SCRIPT_DIR/node-shim.js" > "$EXIT_PROBE"
cat >> "$EXIT_PROBE" <<'PROBE'
try { process.exit(3); } catch (e) {}
console.log("UNREACHABLE");
PROBE

check_exit() {  # <label> <cmd...>
    local label="$1"; shift
    local rc=0
    "$@" "$EXIT_PROBE" > "$TMP/$label-exit.out" 2>&1 || rc=$?
    if [ "$rc" -ne 3 ]; then
        echo "FAIL: $label process.exit(3) exited $rc (expected 3)" >&2
        FAIL=1
    elif grep -q "UNREACHABLE" "$TMP/$label-exit.out"; then
        echo "FAIL: $label process.exit was catchable / did not exit immediately" >&2
        FAIL=1
    else
        echo "  $label: exit 3, uncatchable — OK"
    fi
}

check_exit jsse "$JSSE" --node
command -v node >/dev/null 2>&1 && check_exit node node

# ---- no-host fallback probe -----------------------------------------------
# The library harness always passes --node, but node-shim.js documents a
# degraded pure-JS fallback for plain jsse runs. Cover that path directly so the
# fallback stream never recurses through the shimmed console methods.
echo ""
echo "== jsse fallback without --node =="
FALLBACK_PROBE="$TMP/fallback-probe.js"
cat "$SCRIPT_DIR/node-shim.js" > "$FALLBACK_PROBE"
cat >> "$FALLBACK_PROBE" <<'PROBE'
process.stdout.write("fallback stdout\n");
process.stderr.write("fallback stderr\n");
console.log("fallback console");
console.error("fallback error");
process.stdout.write("partial");
process.exit(0);
PROBE

fallback_rc=0
"$JSSE" "$FALLBACK_PROBE" > "$TMP/jsse-fallback.out" 2> "$TMP/jsse-fallback.err" || fallback_rc=$?
if [ "$fallback_rc" -ne 0 ]; then
    echo "FAIL: jsse fallback without --node exited $fallback_rc" >&2
    [ -s "$TMP/jsse-fallback.out" ] && cat "$TMP/jsse-fallback.out" >&2
    [ -s "$TMP/jsse-fallback.err" ] && cat "$TMP/jsse-fallback.err" >&2
    FAIL=1
elif ! diff -u - "$TMP/jsse-fallback.out" > "$TMP/fallback.diff" <<'EXPECTED'; then
fallback stdout
fallback stderr
fallback console
fallback error
partial
EXPECTED
    echo "FAIL: jsse fallback without --node output differed:" >&2
    cat "$TMP/fallback.diff" >&2
    FAIL=1
elif [ -s "$TMP/jsse-fallback.err" ]; then
    echo "FAIL: jsse fallback without --node wrote stderr:" >&2
    cat "$TMP/jsse-fallback.err" >&2
    FAIL=1
else
    echo "  jsse: fallback stdout/stderr/console without --node — OK"
fi

echo ""
if [ "$FAIL" -eq 0 ]; then
    echo "OK: node-shim self-test passed on jsse (cross-checked against Node)"
    exit 0
fi
echo "FAILED: node-shim self-test" >&2
exit 1
