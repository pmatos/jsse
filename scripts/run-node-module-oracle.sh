#!/usr/bin/env bash
# Cross-check the embedded `module` builtin (issue #1220) against Node.
#
# Usage:
#   ./scripts/run-node-module-oracle.sh [--node] [--no-cross-check]
#
# scripts/module-fixtures/proj holds two drivers. main.js prints one normalised
# line per scenario for the static API of `module` (descriptors, builtinModules,
# isBuiltin, wrap/wrapper, createRequire, findPackageJSON, and the hooks tools
# patch: _extensions, _resolveFilename, _load, _compile, _preloadModules). sm.js
# does the same for SourceMap, findSourceMap, get/setSourceMapsSupport and
# process.setSourceMapsEnabled over the files in proj/sm. Both must produce
# byte-identical output on jsse (--node) and on Node. Cases where jsse knowingly
# differs (a real compile cache, stripTypeScriptTypes) are not part of the tree.
#
# The Node output is pinned in scripts/module-fixtures/expected-{main,sm}.txt,
# which tests/node_module.rs compares jsse against without needing Node.
#
# Options:
#   --node             run on Node only and rewrite the pinned expectations
#   --no-cross-check   run on jsse only, diffed against the pinned expectations

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TREE="$PROJECT_DIR/scripts/module-fixtures/proj"
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

export NODE_DISABLE_COMPILE_CACHE=1
unset NODE_OPTIONS

FAIL=0
for kind in main sm; do
    if [ "$NODE_ONLY" -eq 0 ]; then
        rc=0
        (cd "$TREE" && "$JSSE" --node "$kind.js") > "$WORK/jsse.$kind.out" 2> "$WORK/jsse.$kind.err" || rc=$?
        if [ "$rc" -ne 0 ]; then
            echo "FAIL $kind: jsse --node exit $rc"
            head -10 "$WORK/jsse.$kind.err" | sed 's/^/  stderr: /'
            FAIL=1
        fi
    fi
    if [ "$CROSS_CHECK" -eq 1 ] || [ "$NODE_ONLY" -eq 1 ]; then
        rc=0
        (cd "$TREE" && "$NODE_BIN" "$kind.js") > "$WORK/node.$kind.out" 2> "$WORK/node.$kind.err" || rc=$?
        if [ "$rc" -ne 0 ]; then
            echo "FAIL $kind: node exit $rc"
            head -10 "$WORK/node.$kind.err" | sed 's/^/  stderr: /'
            FAIL=1
        fi
    fi
    want="$PROJECT_DIR/scripts/module-fixtures/expected-$kind.txt"
    if [ "$NODE_ONLY" -eq 1 ]; then
        cp "$WORK/node.$kind.out" "$want"
        echo "wrote $want ($(wc -l < "$want") lines)"
        continue
    fi
    if [ "$CROSS_CHECK" -eq 1 ] && ! diff -u "$WORK/node.$kind.out" "$WORK/jsse.$kind.out" > "$WORK/diff.txt"; then
        echo "FAIL $kind: output differs (--- node, +++ jsse)"
        head -60 "$WORK/diff.txt"
        FAIL=1
    elif ! diff -u "$want" "$WORK/jsse.$kind.out" > "$WORK/diff.txt"; then
        echo "FAIL $kind: output differs from the pinned $(basename "$want") (--- pinned, +++ jsse)"
        head -60 "$WORK/diff.txt"
        FAIL=1
    else
        echo "ok   $kind: $(wc -l < "$WORK/jsse.$kind.out") lines identical to the pinned output and, when run, to node"
    fi
done

if [ "$FAIL" -ne 0 ]; then
    echo "node-module oracle: FAILED"
    exit 1
fi
echo "node-module oracle: all passed"
