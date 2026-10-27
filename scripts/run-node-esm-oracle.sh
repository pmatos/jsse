#!/usr/bin/env bash
# Cross-check the ES module loader (issue #1240) against Node.
#
# Usage:
#   ./scripts/run-node-esm-oracle.sh [--node] [--no-cross-check]
#
# scripts/esm-fixtures/proj is a project tree mixing .mjs, .cjs and
# "type": "module" files, packages with conditional exports and patterns,
# "imports" maps, self references, JSON modules, builtins, top-level await and
# dynamic import() from ES modules and CommonJS. Its main.mjs prints one
# normalised line per scenario (errors as name/code/first message line, paths
# relative to the tree) and must produce byte-identical output on jsse (--node)
# and on Node. Cases where jsse knowingly differs (data: URLs) are not part of the
# tree; import.meta and CommonJS interop are in run-node-esm-interop-oracle.sh.
#
# The Node output is pinned in scripts/esm-fixtures/expected-main.txt, which
# tests/node_esm.rs compares jsse against without needing Node.
#
# Options:
#   --node             run on Node only and rewrite the pinned expectation
#   --no-cross-check   run on jsse only, diffed against the pinned expectation

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
MAIN="$PROJECT_DIR/scripts/esm-fixtures/proj/main.mjs"
WANT="$PROJECT_DIR/scripts/esm-fixtures/expected-main.txt"
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
if [ "$NODE_ONLY" -eq 0 ]; then
    rc=0
    (cd "$PROJECT_DIR" && "$JSSE" --node "$MAIN") > "$WORK/jsse.out" 2> "$WORK/jsse.err" || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "FAIL: jsse --node exit $rc"
        head -10 "$WORK/jsse.err" | sed 's/^/  stderr: /'
        FAIL=1
    fi
fi
if [ "$CROSS_CHECK" -eq 1 ] || [ "$NODE_ONLY" -eq 1 ]; then
    rc=0
    (cd "$PROJECT_DIR" && "$NODE_BIN" "$MAIN") > "$WORK/node.out" 2> "$WORK/node.err" || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "FAIL: node exit $rc"
        head -10 "$WORK/node.err" | sed 's/^/  stderr: /'
        FAIL=1
    fi
fi
if [ "$NODE_ONLY" -eq 1 ]; then
    cp "$WORK/node.out" "$WANT"
    echo "wrote $WANT ($(wc -l < "$WANT") lines)"
    exit "$FAIL"
fi
if [ "$CROSS_CHECK" -eq 1 ] && ! diff -u "$WORK/node.out" "$WORK/jsse.out" > "$WORK/diff.txt"; then
    echo "FAIL: output differs (--- node, +++ jsse)"
    head -60 "$WORK/diff.txt"
    FAIL=1
elif ! diff -u "$WANT" "$WORK/jsse.out" > "$WORK/diff.txt"; then
    echo "FAIL: output differs from the pinned $(basename "$WANT") (--- pinned, +++ jsse)"
    head -60 "$WORK/diff.txt"
    FAIL=1
else
    echo "ok: $(wc -l < "$WORK/jsse.out") lines identical to the pinned output and, when run, to node"
fi

if [ "$FAIL" -ne 0 ]; then
    echo "node-esm oracle: FAILED"
    exit 1
fi
echo "node-esm oracle: all passed"
