#!/usr/bin/env bash
# Run the Node host-compat shim fixtures on jsse (and Node as a reference).
#
# Usage:
#   ./scripts/run-shim-fixtures.sh [--node] [--no-cross-check]
#
# Each fixture in scripts/shim-fixtures/*.fixture.js is a self-verifying test
# for the shared shims (node-shim.js + node-buffer-shim.js). The runner
# prepends both shims to the fixture and runs the result on:
#   - jsse (release) — the shims define Buffer/TextEncoder/TextDecoder
#   - Node           — the shims are inert; the fixture exercises native APIs
# A fixture passes iff the engine exits 0. By default the two engines must also
# report the same "N of N assertions passed" count, so a fixture silently
# skipping checks on jsse cannot masquerade as a pass.
#
# Options:
#   --node             run on Node only (reference / debugging)
#   --no-cross-check   run on jsse only, skip the Node comparison

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
FIXTURE_DIR="$SCRIPT_DIR/shim-fixtures"
JSSE="$PROJECT_DIR/target/release/jsse"
SHIMS=("$SCRIPT_DIR/node-shim.js" "$SCRIPT_DIR/node-buffer-shim.js")
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

if [ "$NODE_ONLY" -eq 0 ] && [ ! -x "$JSSE" ]; then
    echo "Building jsse (release)..."
    (cd "$PROJECT_DIR" && cargo build --release)
fi

# Parse "SHIM-FIXTURE: X of Y assertions passed"; echoes the count or empty.
parse_count() { grep -oE 'SHIM-FIXTURE: [0-9]+ of [0-9]+' "$1" | tail -1 | grep -oE '[0-9]+ of' | grep -oE '^[0-9]+' || true; }

# <engine> <fixture-bundle> <label> → returns 0 on pass, sets COUNT
COUNT=""
run_one() {
    local engine="$1" bundle="$2" label="$3"
    local out="$WORK/out-$label.txt" rc=0
    "$engine" "$bundle" > "$out" 2>&1 || rc=$?
    cat "$out"
    COUNT="$(parse_count "$out")"
    if [ "$rc" -ne 0 ]; then
        echo "  → $label: FAIL (exit $rc)"
        return 1
    fi
    # A clean exit with no assertion-count marker means the fixture never
    # reached its report line (e.g. an accidental early return) — that must be a
    # failure, not a silent green with zero verified assertions.
    if [ -z "$COUNT" ]; then
        echo "  → $label: FAIL (no SHIM-FIXTURE count reported)"
        return 1
    fi
    echo "  → $label: PASS ($COUNT assertions)"
    return 0
}

shopt -s nullglob
FIXTURES=("$FIXTURE_DIR"/*.fixture.js)
if [ "${#FIXTURES[@]}" -eq 0 ]; then
    echo "No fixtures found in $FIXTURE_DIR" >&2
    exit 2
fi

FAIL=0
for fixture in "${FIXTURES[@]}"; do
    name="$(basename "$fixture")"
    bundle="$WORK/bundle-$name"
    cat "${SHIMS[@]}" "$fixture" > "$bundle"
    echo "========================================"
    echo "  Fixture: $name"
    echo "========================================"

    if [ "$NODE_ONLY" -eq 1 ]; then
        run_one node "$bundle" node || FAIL=1
        continue
    fi

    JSSE_COUNT=""
    run_one "$JSSE" "$bundle" jsse && JSSE_COUNT="$COUNT" || FAIL=1

    if [ "$CROSS_CHECK" -eq 1 ]; then
        if command -v node >/dev/null 2>&1; then
            NODE_COUNT=""
            run_one node "$bundle" node && NODE_COUNT="$COUNT" || FAIL=1
            if [ -n "$JSSE_COUNT" ] && [ "$JSSE_COUNT" != "$NODE_COUNT" ]; then
                echo "  MISMATCH: jsse ran $JSSE_COUNT assertions, Node ran $NODE_COUNT" >&2
                FAIL=1
            fi
        else
            echo "  WARNING: node not found — skipping cross-check" >&2
        fi
    fi
done

echo "========================================"
if [ "$FAIL" -eq 0 ]; then
    echo "OK: all shim fixtures passed"
    exit 0
fi
echo "FAILED: shim fixtures" >&2
exit 1
