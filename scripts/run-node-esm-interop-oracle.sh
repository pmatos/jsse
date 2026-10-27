#!/usr/bin/env bash
# Cross-check import.meta, ES module/CommonJS interop and require(esm)
# (issue #1241) against Node.
#
# Usage:
#   ./scripts/run-node-esm-interop-oracle.sh [--node] [--no-cross-check]
#
# scripts/esm-interop-fixtures/proj holds three drivers, each of which prints
# one normalised line per scenario and must produce byte-identical output on
# jsse (--node) and on Node:
#   main.mjs  import.meta (keys, url, resolve), createRequire(import.meta.url),
#             named exports detected in CommonJS modules, default and
#             module.exports, snapshot versus live bindings, require() of ES
#             modules from an ES module and their error codes
#   main.cjs  require(esm): the namespace facade with __esModule, the
#             'module.exports' export, live bindings, ERR_REQUIRE_ASYNC_MODULE,
#             ERR_REQUIRE_CYCLE_MODULE, the module-sync condition, typeless
#             files with ES syntax
#   lex.mjs   60 CommonJS source snippets and the export names Node detects
#
# Node's output is pinned in scripts/esm-interop-fixtures/expected-*.txt, which
# tests/node_esm_interop.rs compares jsse against without Node.
#
# Options:
#   --node             run on Node only and rewrite the pinned expectations
#   --no-cross-check   run on jsse only, diffed against the pinned expectations

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TREE="$PROJECT_DIR/scripts/esm-interop-fixtures"
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
unset NODE_OPTIONS

FAIL=0
for driver in main.mjs main.cjs lex.mjs; do
    name="${driver%.*}-${driver##*.}"
    main="$TREE/proj/$driver"
    want="$TREE/expected-$name.txt"
    if [ "$NODE_ONLY" -eq 0 ]; then
        rc=0
        (cd "$PROJECT_DIR" && "$JSSE" --node "$main") > "$WORK/jsse.out" 2> "$WORK/jsse.err" || rc=$?
        if [ "$rc" -ne 0 ]; then
            echo "FAIL: $driver: jsse --node exit $rc"
            head -10 "$WORK/jsse.err" | sed 's/^/  stderr: /'
            FAIL=1
        fi
    fi
    if [ "$CROSS_CHECK" -eq 1 ] || [ "$NODE_ONLY" -eq 1 ]; then
        rc=0
        (cd "$PROJECT_DIR" && "$NODE_BIN" "$main") > "$WORK/node.out" 2> "$WORK/node.err" || rc=$?
        if [ "$rc" -ne 0 ]; then
            echo "FAIL: $driver: node exit $rc"
            head -10 "$WORK/node.err" | sed 's/^/  stderr: /'
            FAIL=1
        fi
    fi
    if [ "$NODE_ONLY" -eq 1 ]; then
        cp "$WORK/node.out" "$want"
        echo "wrote $want ($(wc -l < "$want") lines)"
        continue
    fi
    if [ "$CROSS_CHECK" -eq 1 ] && ! diff -u "$WORK/node.out" "$WORK/jsse.out" > "$WORK/diff.txt"; then
        echo "FAIL: $driver: output differs (--- node, +++ jsse)"
        head -60 "$WORK/diff.txt"
        FAIL=1
    elif ! diff -u "$want" "$WORK/jsse.out" > "$WORK/diff.txt"; then
        echo "FAIL: $driver: output differs from the pinned $(basename "$want") (--- pinned, +++ jsse)"
        head -60 "$WORK/diff.txt"
        FAIL=1
    else
        echo "ok: $driver: $(wc -l < "$WORK/jsse.out") lines identical to the pinned output and, when run, to node"
    fi
done

if [ "$FAIL" -ne 0 ]; then
    echo "node-esm-interop oracle: FAILED"
    exit 1
fi
echo "node-esm-interop oracle: all passed"
