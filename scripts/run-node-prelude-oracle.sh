#!/usr/bin/env bash
# Cross-check the Node prelude embedded in `jsse --node` (issue #1216) against
# Node, with nothing prepended to the scripts.
#
# Usage:
#   ./scripts/run-node-prelude-oracle.sh [--node] [--no-cross-check]
#
# Each scripts/prelude-fixtures/*.fixture.js is run as a plain script on jsse
# (`--node`) and on Node. A fixture passes iff both exit 0 and produce
# byte-identical stdout and stderr. Then the opt-in gate is checked: without
# `--node` (and with `--prelude`, which test262 uses) none of the prelude's
# globals may exist.
#
# Options:
#   --node             run on Node only (reference / debugging)
#   --no-cross-check   run on jsse only (exit 0 required, no diff against Node)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
FIXTURE_DIR="$SCRIPT_DIR/prelude-fixtures"
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

for fixture in "${FIXTURES[@]}"; do
    name="$(basename "$fixture" .fixture.js)"
    jsse_ok=1
    if [ "$NODE_ONLY" -eq 0 ]; then
        rc=0
        "$JSSE" --node "$fixture" > "$WORK/jsse.out" 2> "$WORK/jsse.err" || rc=$?
        if [ "$rc" -ne 0 ]; then
            echo "FAIL $name: jsse --node exit $rc"
            sed 's/^/  stderr: /' "$WORK/jsse.err" | head -5
            FAIL=1
            continue
        fi
    fi
    if [ "$CROSS_CHECK" -eq 1 ] || [ "$NODE_ONLY" -eq 1 ]; then
        rc=0
        "$NODE_BIN" "$fixture" > "$WORK/node.out" 2> "$WORK/node.err" || rc=$?
        if [ "$rc" -ne 0 ]; then
            echo "FAIL $name: node exit $rc"
            FAIL=1
            continue
        fi
        if [ "$NODE_ONLY" -eq 0 ]; then
            for stream in out err; do
                if ! diff -u "$WORK/node.$stream" "$WORK/jsse.$stream" > "$WORK/diff.txt"; then
                    echo "FAIL $name: std$stream differs (--- node, +++ jsse)"
                    cat "$WORK/diff.txt"
                    jsse_ok=0
                fi
            done
            [ "$jsse_ok" -eq 1 ] || { FAIL=1; continue; }
        fi
    fi
    echo "ok   $name"
done

if [ "$NODE_ONLY" -eq 0 ]; then
    GATE='print(["process","Buffer","TextEncoder","TextDecoder","crypto","util"].map(function (n) { return n + ":" + typeof globalThis[n]; }).join(" "), typeof console.group, typeof __host_write, typeof __node_builtin, typeof __host_builtin_source)'
    EXPECT='process:undefined Buffer:undefined TextEncoder:undefined TextDecoder:undefined crypto:undefined util:undefined undefined undefined undefined undefined'
    echo 'void 0;' > "$WORK/empty-prelude.js"
    for mode in plain prelude; do
        args=()
        [ "$mode" = prelude ] && args=(--prelude "$WORK/empty-prelude.js")
        got="$("$JSSE" "${args[@]}" -e "$GATE" 2>&1 || true)"
        if [ "$got" != "$EXPECT" ]; then
            echo "FAIL gate ($mode): prelude globals visible without --node: $got"
            FAIL=1
        else
            echo "ok   gate ($mode): prelude absent without --node"
        fi
    done

    one_liner="$("$JSSE" --node -e 'console.log(process.version, Buffer.from("a"))' 2>&1 || true)"
    if ! [[ "$one_liner" =~ ^v[0-9]+\.[0-9]+\.[0-9]+\  ]]; then
        echo "FAIL one-liner: got '$one_liner'"
        FAIL=1
    else
        echo "ok   one-liner: jsse --node -e works with no prepended files"
    fi
fi

if [ "$FAIL" -ne 0 ]; then
    echo "node-prelude oracle: FAILED"
    exit 1
fi
echo "node-prelude oracle: all passed"
