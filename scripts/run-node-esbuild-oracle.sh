#!/usr/bin/env bash
# Run the esbuild JS API (the version the TypeScript v6.0.3 repo pins) on `jsse --node` and diff it
# against Node (issue #1252). The esbuild package is not vendored.
#
# Usage:
#   ESBUILD_DIR=/path/to/node_modules/esbuild [TS_SRC_DIR=/path/to/TypeScript] \
#     ./scripts/run-node-esbuild-oracle.sh [--no-cross-check] [--node] [--pin]
#
# ESBUILD_DIR is the installed `esbuild` package with its platform binary next to it
# (@esbuild/linux-x64 in the sibling node_modules directory, or ESBUILD_BINARY_PATH). Without it
# the script prints a notice and exits 0. TS_SRC_DIR is a TypeScript checkout at the pinned tag with
# generated diagnostics (see ts-suite/); it enables the typescript-bundle fixture, which bundles
# src/typescript with the Herebyfile's options. Without it that fixture is skipped.
#
# Each scripts/esbuild-fixtures/*.fixture.js runs in a fresh copy of scripts/esbuild-fixtures/project
# on both engines, with the esbuild service child wrapped so its pid is recorded. Header lines:
#   // RUN: <exit-status>     expected exit status (every engine)
#   // STDERR: same           stderr (with the scratch path scrubbed) must match too
#   // NEEDS_ENV: <name>      skip when the variable is unset
# A fixture passes iff stdout and exit status match Node's and the RUN line, nothing is left
# running when it ends (a hang is a timeout failure) and every service child has exited.
#
# Options:
#   --node             run on Node only (reference / debugging)
#   --no-cross-check   run on jsse only and compare stdout with expected-sha256.txt
#   --pin              rewrite expected-sha256.txt from Node's stdout (esbuild 0.27.3)
# ONLY=<fixture> runs a single fixture.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
FIXTURE_DIR="$SCRIPT_DIR/esbuild-fixtures"
PINNED="$FIXTURE_DIR/expected-sha256.txt"
JSSE="${JSSE:-$PROJECT_DIR/target/release/jsse}"
case "$JSSE" in /*) ;; *) JSSE="$PWD/$JSSE" ;; esac
PINNED_VERSION=0.27.3
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

NODE_ONLY=0
CROSS_CHECK=1
PIN=0
for arg in "$@"; do
    case "$arg" in
        --node) NODE_ONLY=1 ;;
        --no-cross-check) CROSS_CHECK=0 ;;
        --pin) PIN=1 ;;
        *) echo "unknown option: $arg" >&2; exit 2 ;;
    esac
done

if [ -z "${ESBUILD_DIR:-}" ]; then
    echo "SKIP: ESBUILD_DIR is not set; point it at an installed esbuild $PINNED_VERSION package to run the esbuild oracle"
    exit 0
fi
ESBUILD_DIR="$(cd "$ESBUILD_DIR" 2>/dev/null && pwd || echo "$ESBUILD_DIR")"
if [ ! -f "$ESBUILD_DIR/lib/main.js" ]; then
    echo "SKIP: $ESBUILD_DIR/lib/main.js does not exist (ESBUILD_DIR must be the esbuild package directory)"
    exit 0
fi
NODE_BIN="$(command -v node || true)"
if [ -z "$NODE_BIN" ]; then
    echo "SKIP: node not found; the esbuild oracle needs it to read the package version"
    exit 0
fi
VERSION="$("$NODE_BIN" -p 'require(process.argv[1] + "/package.json").version' "$ESBUILD_DIR")"
if [ "$VERSION" != "$PINNED_VERSION" ]; then
    echo "SKIP: the esbuild oracle targets esbuild $PINNED_VERSION, found $VERSION"
    exit 0
fi
BINARY="${ESBUILD_BINARY_PATH:-}"
if [ -z "$BINARY" ]; then
    for candidate in "$ESBUILD_DIR"/../@esbuild/*/bin/esbuild "$ESBUILD_DIR"/node_modules/@esbuild/*/bin/esbuild; do
        [ -x "$candidate" ] && BINARY="$candidate" && break
    done
fi
if [ -z "$BINARY" ] || [ ! -x "$BINARY" ]; then
    echo "SKIP: no esbuild platform binary next to $ESBUILD_DIR (install @esbuild/<platform> or set ESBUILD_BINARY_PATH)"
    exit 0
fi
if [ "$CROSS_CHECK" -eq 1 ] && [ "$NODE_ONLY" -eq 0 ] && [ -z "$NODE_BIN" ]; then
    CROSS_CHECK=0
fi
if [ "$NODE_ONLY" -eq 0 ] && [ ! -x "$JSSE" ]; then
    echo "Building jsse (release)..."
    (cd "$PROJECT_DIR" && cargo build --release)
fi
echo "esbuild $VERSION from $ESBUILD_DIR (binary: $BINARY, jsse: $JSSE)"

SERVICE_PIDS="$WORK/service-pids"
: > "$SERVICE_PIDS"
WRAPPER="$WORK/esbuild-wrapper"
cat > "$WRAPPER" <<WRAP
#!/bin/sh
case "\$*" in *--service*) echo \$\$ >> "$SERVICE_PIDS" ;; esac
exec "$BINARY" "\$@"
WRAP
chmod +x "$WRAPPER"

# run_engine <out-prefix> <dir> <fixture> <engine...>: stdout/stderr into <prefix>.out/.err, sets RC
run_engine() {
    local prefix="$1" dir="$2" fixture="$3"
    shift 3
    rm -rf "$dir"
    cp -r "$FIXTURE_DIR/project" "$dir"
    RC=0
    (cd "$dir" && env -u NODE_OPTIONS -u FORCE_COLOR -u NO_COLOR -u COLUMNS \
        ESBUILD_DIR="$ESBUILD_DIR" ESBUILD_BINARY_PATH="$WRAPPER" timeout 180 "$@" "$fixture") \
        > "$prefix.out" 2> "$prefix.err" || RC=$?
    sed -i -e "s|$dir|<cwd>|g" -e 's/Done in [0-9]*ms/Done in Nms/' "$prefix.err"
}

wait_gone() {
    local pid="$1" tries=0
    while kill -0 "$pid" 2> /dev/null && [ "$tries" -lt 100 ]; do
        sleep 0.1
        tries=$((tries + 1))
    done
    ! kill -0 "$pid" 2> /dev/null
}

declare -A PIN_LINES
if [ -f "$PINNED" ]; then
    while read -r name digest; do PIN_LINES[$name]="$digest"; done < "$PINNED"
fi

FAIL=0
shopt -s nullglob
FIXTURES=("$FIXTURE_DIR"/*.fixture.js)
for fixture in "${FIXTURES[@]}"; do
    [ -n "${ONLY:-}" ] && [ "$(basename "$fixture" .fixture.js)" != "$ONLY" ] && continue
    name="$(basename "$fixture" .fixture.js)"
    expect="$(sed -n 's|^// RUN: *||p' "$fixture" | head -1)"
    needs_env="$(sed -n 's|^// NEEDS_ENV: *||p' "$fixture" | head -1)"
    if [ -n "$needs_env" ] && [ -z "${!needs_env:-}" ]; then
        echo "skip $name (needs \$$needs_env)"
        continue
    fi
    stderr_same="$(sed -n 's|^// STDERR: *||p' "$fixture" | head -1)"
    engines=()
    [ "$NODE_ONLY" -eq 0 ] && engines+=(jsse)
    { [ "$CROSS_CHECK" -eq 1 ] || [ "$NODE_ONLY" -eq 1 ] || [ "$PIN" -eq 1 ]; } && engines+=(node)
    ok=1
    : > "$SERVICE_PIDS"
    for engine in "${engines[@]}"; do
        if [ "$engine" = jsse ]; then
            run_engine "$WORK/$engine" "$WORK/$engine.d" "$fixture" "$JSSE" --node
        else
            run_engine "$WORK/$engine" "$WORK/$engine.d" "$fixture" "$NODE_BIN"
        fi
        if [ "$RC" -ne "$expect" ]; then
            echo "FAIL $name: $engine exit $RC, expected $expect"
            sed 's/^/  stderr: /' "$WORK/$engine.err" | head -8
            ok=0
        fi
    done
    while read -r pid; do
        [ -n "$pid" ] && ! wait_gone "$pid" && { echo "FAIL $name: esbuild service pid $pid still running after the fixture ended"; kill "$pid" 2> /dev/null || true; ok=0; }
    done < "$SERVICE_PIDS"
    if [ "$ok" -eq 1 ] && [ "$CROSS_CHECK" -eq 1 ] && [ "${#engines[@]}" -eq 2 ]; then
        if ! diff -u "$WORK/node.out" "$WORK/jsse.out" > "$WORK/diff.txt"; then
            echo "FAIL $name: stdout differs (--- node, +++ jsse)"
            head -40 "$WORK/diff.txt"
            ok=0
        elif [ "$stderr_same" = same ] && ! diff -u "$WORK/node.err" "$WORK/jsse.err" > "$WORK/diff.txt"; then
            echo "FAIL $name: stderr differs (--- node, +++ jsse)"
            head -40 "$WORK/diff.txt"
            ok=0
        fi
    fi
    if [ "$ok" -eq 1 ] && [ "$CROSS_CHECK" -eq 0 ] && [ "$NODE_ONLY" -eq 0 ] && [ "$PIN" -eq 0 ] && [ -n "${PIN_LINES[$name]:-}" ]; then
        got="$(sha256sum < "$WORK/jsse.out" | cut -d' ' -f1)"
        if [ "$got" != "${PIN_LINES[$name]}" ]; then
            echo "FAIL $name: stdout digest $got differs from the pinned ${PIN_LINES[$name]}"
            ok=0
        fi
    fi
    if [ "$ok" -eq 1 ] && [ "$PIN" -eq 1 ]; then
        PIN_LINES[$name]="$(sha256sum < "$WORK/node.out" | cut -d' ' -f1)"
    fi
    if [ "$ok" -eq 1 ]; then
        echo "ok   $name (exit $expect)"
    else
        FAIL=1
    fi
done

if [ "$PIN" -eq 1 ] && [ "$FAIL" -eq 0 ]; then
    for name in $(printf '%s\n' "${!PIN_LINES[@]}" | sort); do
        echo "$name ${PIN_LINES[$name]}"
    done > "$PINNED"
    echo "pinned $PINNED"
fi

if [ "$FAIL" -ne 0 ]; then
    echo "node-esbuild oracle: FAILED"
    exit 1
fi
echo "node-esbuild oracle: all passed"
