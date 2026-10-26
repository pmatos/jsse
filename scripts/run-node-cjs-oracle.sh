#!/usr/bin/env bash
# Cross-check the CommonJS loader (issue #1219) against Node.
#
# Usage:
#   ./scripts/run-node-cjs-oracle.sh [--node] [--no-cross-check]
#
# scripts/cjs-fixtures/proj is a project tree (relative/absolute/directory/JSON
# requires, a node_modules tree with exports and imports maps, self references,
# cycles, NODE_PATH and a $HOME/.node_modules). Its main.js prints one
# normalised line per scenario (errors as code/message/requireStack, paths
# relative to the tree) and must produce byte-identical output on jsse (--node)
# and on Node; eval.js does the same for `-e`. Cases where jsse knowingly
# differs (require() of ES modules, native addons) are not part of the tree.
#
# The Node output is pinned in scripts/cjs-fixtures/expected-{main,eval}.txt,
# which tests/node_cjs.rs compares jsse against without needing Node.
#
# Options:
#   --node             run on Node only and rewrite the pinned expectations
#   --no-cross-check   run on jsse only, diffed against the pinned expectations

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TREE="$PROJECT_DIR/scripts/cjs-fixtures/proj"
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

export HOME="$TREE/home"
export NODE_PATH="$TREE/np"
unset NODE_OPTIONS


FAIL=0
for kind in main eval; do
    if [ "$kind" = main ]; then
        node_args=("$TREE/main.js"); jsse_args=(--node "$TREE/main.js")
    else
        src="$(cat "$PROJECT_DIR/scripts/cjs-fixtures/eval.js")"
        node_args=(-e "$src"); jsse_args=(--node -e "$src")
    fi
    cwd="$PROJECT_DIR"; [ "$kind" = eval ] && cwd="$TREE"
    if [ "$NODE_ONLY" -eq 0 ]; then
        rc=0
        (cd "$cwd" && "$JSSE" "${jsse_args[@]}") > "$WORK/jsse.$kind.out" 2> "$WORK/jsse.$kind.err" || rc=$?
        if [ "$rc" -ne 0 ]; then
            echo "FAIL $kind: jsse --node exit $rc"
            head -10 "$WORK/jsse.$kind.err" | sed 's/^/  stderr: /'
            FAIL=1
        fi
    fi
    if [ "$CROSS_CHECK" -eq 1 ] || [ "$NODE_ONLY" -eq 1 ]; then
        rc=0
        (cd "$cwd" && "$NODE_BIN" "${node_args[@]}") > "$WORK/node.$kind.out" 2> "$WORK/node.$kind.err" || rc=$?
        if [ "$rc" -ne 0 ]; then
            echo "FAIL $kind: node exit $rc"
            head -10 "$WORK/node.$kind.err" | sed 's/^/  stderr: /'
            FAIL=1
        fi
    fi
    want="$PROJECT_DIR/scripts/cjs-fixtures/expected-$kind.txt"
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
    echo "node-cjs oracle: FAILED"
    exit 1
fi
echo "node-cjs oracle: all passed"
