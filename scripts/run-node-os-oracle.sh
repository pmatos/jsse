#!/usr/bin/env bash
# Cross-check the embedded `os` module (issue #1222) against Node on this host.
#
# Usage:
#   ./scripts/run-node-os-oracle.sh [--node] [--no-cross-check]
#
# scripts/os-fixtures/probe.js prints one JSON line per property of `os`: every
# deterministic value (platform, arch, release, hostname, homedir, tmpdir,
# userInfo, networkInterfaces, EOL, constants with descriptors, ...), the shape
# of the module (key order, function name/length/own keys, Symbol.toPrimitive)
# and the structure of the values that move between calls (freemem, uptime,
# loadavg, cpu times). It runs under several environments (HOME unset or empty,
# TMPDIR variants, in-script process.env edits) and jsse's stdout must equal
# Node's byte for byte.
#
# Options:
#   --node             run on Node only
#   --no-cross-check   run on jsse only (exit 0 required)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PROBE="$PROJECT_DIR/scripts/os-fixtures/probe.js"
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

# name|scenario|env assignments (env -u NAME unsets, NAME=value sets)
SCENARIOS=(
    "default|default|"
    "home-unset|default|-u HOME"
    "home-empty|default|HOME="
    "home-custom|default|HOME=/custom/home/"
    "tmpdir-trailing|default|TMPDIR=/x//"
    "tmpdir-root|default|TMPDIR=/"
    "tmpdir-empty-falls-to-tmp|default|TMPDIR= TMP=/from-tmp/"
    "tmpdir-temp|default|-u TMPDIR -u TMP TEMP=/from-temp"
    "tmpdir-none|default|-u TMPDIR -u TMP -u TEMP"
    "edit-env|edit|"
)

FAIL=0
for entry in "${SCENARIOS[@]}"; do
    IFS='|' read -r name scenario envspec <<< "$entry"
    read -r -a env_args <<< "$envspec"
    status="ok  "
    if [ "$NODE_ONLY" -eq 0 ]; then
        rc=0
        env "${env_args[@]}" "$JSSE" --node "$PROBE" "$scenario" > "$WORK/jsse.out" 2> "$WORK/jsse.err" || rc=$?
        if [ "$rc" -ne 0 ]; then
            echo "FAIL $name: jsse --node exit $rc"
            head -5 "$WORK/jsse.err" | sed 's/^/  stderr: /'
            status="FAIL"; FAIL=1
        fi
    fi
    if [ "$CROSS_CHECK" -eq 1 ] || [ "$NODE_ONLY" -eq 1 ]; then
        rc=0
        env "${env_args[@]}" "$NODE_BIN" "$PROBE" "$scenario" > "$WORK/node.out" 2> "$WORK/node.err" || rc=$?
        if [ "$rc" -ne 0 ]; then
            echo "FAIL $name: node exit $rc"
            head -5 "$WORK/node.err" | sed 's/^/  stderr: /'
            status="FAIL"; FAIL=1
        elif [ "$NODE_ONLY" -eq 0 ] && [ "$status" != "FAIL" ]; then
            if ! diff -u "$WORK/node.out" "$WORK/jsse.out" > "$WORK/diff.txt"; then
                echo "FAIL $name: stdout differs (--- node, +++ jsse)"
                head -40 "$WORK/diff.txt"
                status="FAIL"; FAIL=1
            fi
        fi
    fi
    echo "$status $name"
done

if [ "$FAIL" -ne 0 ]; then
    echo "node-os oracle: FAILED"
    exit 1
fi
echo "node-os oracle: all passed"
