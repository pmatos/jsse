#!/usr/bin/env bash
# Cross-check the `fs` of `jsse --node` (issues #1217, #1218, #1234) against Node
# on a fixture directory tree.
#
# Usage:
#   ./scripts/run-node-fs-oracle.sh [--node] [--no-cross-check]
#
# Each scripts/fs-fixtures/*.fixture.js runs on jsse (`--node`) and on Node with
# FS_ORACLE_DIR pointing at a fresh, identically seeded directory (a target
# file, a real directory and symlinks); paths are printed relative to it. A
# fixture passes iff both exit 0 and produce byte-identical stdout and stderr.
# Then the opt-in gate is checked: without `--node` no fs floor primitive or
# registry may exist.
#
# Options:
#   --node             run on Node only (reference / debugging)
#   --no-cross-check   run on jsse only (exit 0 required, no diff against Node)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
FIXTURE_DIR="$SCRIPT_DIR/fs-fixtures"
JSSE="${JSSE:-$PROJECT_DIR/target/release/jsse}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
umask 022

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

seed_tree() {
    local dir="$1"
    mkdir -p "$dir/real-dir"
    printf 'target' > "$dir/target.txt"
    printf 'inner' > "$dir/real-dir/in.txt"
    ln -s target.txt "$dir/link-file"
    ln -s real-dir "$dir/link-dir"
    ln -s ../target.txt "$dir/real-dir/up"
    ln -s missing "$dir/dangling"
    ln -s loop "$dir/loop"
}

run_engine() {
    local engine="$1" fixture="$2" tag="$3" dir
    dir="$WORK/tree-$tag"
    mkdir -p "$dir"
    dir="$(cd "$dir" && pwd -P)"
    seed_tree "$dir"
    local rc=0
    if [ "$engine" = node ]; then
        FS_ORACLE_DIR="$dir" "$NODE_BIN" "$fixture" > "$WORK/$tag.out" 2> "$WORK/$tag.err" || rc=$?
    else
        FS_ORACLE_DIR="$dir" "$JSSE" --node "$fixture" > "$WORK/$tag.out" 2> "$WORK/$tag.err" || rc=$?
    fi
    return "$rc"
}

FAIL=0
shopt -s nullglob
FIXTURES=("$FIXTURE_DIR"/*.fixture.js)
if [ "${#FIXTURES[@]}" -eq 0 ]; then
    echo "No fixtures found in $FIXTURE_DIR" >&2
    exit 2
fi

for fixture in "${FIXTURES[@]}"; do
    name="$(basename "$fixture" .fixture.js)"
    if [ "$NODE_ONLY" -eq 0 ]; then
        rc=0
        run_engine jsse "$fixture" "jsse-$name" || rc=$?
        if [ "$rc" -ne 0 ]; then
            echo "FAIL $name: jsse --node exit $rc"
            sed 's/^/  stderr: /' "$WORK/jsse-$name.err" | head -5
            FAIL=1
            continue
        fi
    fi
    if [ "$CROSS_CHECK" -eq 1 ] || [ "$NODE_ONLY" -eq 1 ]; then
        rc=0
        run_engine node "$fixture" "node-$name" || rc=$?
        if [ "$rc" -ne 0 ]; then
            echo "FAIL $name: node exit $rc"
            sed 's/^/  stderr: /' "$WORK/node-$name.err" | head -5
            FAIL=1
            continue
        fi
        if [ "$NODE_ONLY" -eq 0 ]; then
            same=1
            for stream in out err; do
                if ! diff -u "$WORK/node-$name.$stream" "$WORK/jsse-$name.$stream" > "$WORK/diff.txt"; then
                    echo "FAIL $name: std$stream differs (--- node, +++ jsse)"
                    cat "$WORK/diff.txt"
                    same=0
                fi
            done
            [ "$same" -eq 1 ] || { FAIL=1; continue; }
        fi
    fi
    echo "ok   $name"
done

if [ "$NODE_ONLY" -eq 0 ]; then
    GATE='print(typeof __host_fs_open, typeof __host_fs_read_file, typeof __host_fs_async, typeof __node_builtins, typeof __node_fs_support)'
    EXPECT='undefined undefined undefined undefined undefined'
    echo 'void 0;' > "$WORK/empty-prelude.js"
    for mode in plain prelude; do
        args=()
        [ "$mode" = prelude ] && args=(--prelude "$WORK/empty-prelude.js")
        got="$("$JSSE" "${args[@]}" -e "$GATE" 2>&1 || true)"
        if [ "$got" != "$EXPECT" ]; then
            echo "FAIL gate ($mode): fs surface visible without --node: $got"
            FAIL=1
        else
            echo "ok   gate ($mode): fs absent without --node"
        fi
    done
    got="$("$JSSE" --node -e 'print(typeof __host_fs_open, typeof __host_fs_async, typeof __node_fs_support, Object.keys(globalThis).filter(function (k) { return /fs|builtins/.test(k); }).length)' 2>&1 || true)"
    if [ "$got" != "undefined undefined undefined 0" ]; then
        echo "FAIL gate (--node): fs floor leaked into script scope: $got"
        FAIL=1
    else
        echo "ok   gate (--node): fs floor is private"
    fi
    got="$("$JSSE" --node -e 'var f = require("fs"); print(typeof f.cpSync, require("node:fs") === f, typeof f.rmSync)' 2>&1 || true)"
    if [ "$got" != "function true function" ]; then
        echo "FAIL gate (--node): fs is not served by require: $got"
        FAIL=1
    else
        echo "ok   gate (--node): require('fs') and node:fs serve the same module"
    fi
fi

if [ "$FAIL" -ne 0 ]; then
    echo "node-fs oracle: FAILED"
    exit 1
fi
echo "node-fs oracle: all passed"
