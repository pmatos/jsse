#!/usr/bin/env bash
# Cross-check the embedded web streams (issue #1360) against Node.
#
# Usage:
#   ./scripts/run-node-webstreams-oracle.sh [--node] [--no-cross-check] [--only NAME]
#
# 1. Every test in scripts/webstreams-fixtures/manifest.txt (Node's own
#    test/parallel files, verbatim) is wrapped by the stream fixtures'
#    harness-assert.js and harness.js plus webstreams-fixtures/harness.js and
#    must print "stream-fixture: ok" and exit 0 on jsse (`--node
#    --expose-internals`) and on Node.
# 2. scripts/webstreams-fixtures/corpus-*.js (chunk, promise and event order,
#    inspect output, error codes and messages, the Node stream interop) must
#    print byte-identical stdout on both, and its digest must equal
#    scripts/webstreams-fixtures/corpus.digest.
#
# Options:
#   --node             run on Node only (reference / regenerating corpus.digest)
#   --no-cross-check   run on jsse only (exit 0 and the pinned digest required)
#   --only NAME        run one manifest test (no corpus)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DIR="$PROJECT_DIR/scripts/webstreams-fixtures"
STREAM_DIR="$PROJECT_DIR/scripts/stream-fixtures"
JSSE="${JSSE:-$PROJECT_DIR/target/release/jsse}"
LIMIT="${WEBSTREAMS_TEST_TIMEOUT:-120}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

NODE_ONLY=0
CROSS_CHECK=1
ONLY=""
while [ $# -gt 0 ]; do
    case "$1" in
        --node) NODE_ONLY=1 ;;
        --no-cross-check) CROSS_CHECK=0 ;;
        --only) ONLY="${2:?--only needs a test name}"; shift ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
    shift
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

FAIL=0
while read -r name _flags; do
    case "$name" in ""|"#"*|"!") continue ;; esac
    if [ -n "$ONLY" ] && [ "$name" != "$ONLY" ]; then continue; fi
    {
        cat "$STREAM_DIR/harness-assert.js" "$STREAM_DIR/harness.js" "$DIR/harness.js"
        echo "(function (require, __filename, __dirname) {"
        cat "$DIR/$name.js"
        printf '\n})(__sRequire, "%s", "%s");\n' "$WORK/$name.js" "$WORK"
    } > "$WORK/$name.js"
    status="ok  "
    if [ "$NODE_ONLY" -eq 0 ]; then
        rc=0
        timeout "$LIMIT" "$JSSE" --node --expose-internals "$WORK/$name.js" \
            > "$WORK/jsse.out" 2> "$WORK/jsse.err" < /dev/null || rc=$?
        if [ "$rc" -ne 0 ] || ! grep -qxF "stream-fixture: ok" "$WORK/jsse.out"; then
            echo "FAIL $name: jsse --node exit $rc"
            head -8 "$WORK/jsse.err" | sed 's/^/  stderr: /'
            status="FAIL"; FAIL=1
        fi
    fi
    if [ "$CROSS_CHECK" -eq 1 ] || [ "$NODE_ONLY" -eq 1 ]; then
        rc=0
        timeout "$LIMIT" "$NODE_BIN" --expose-internals --expose-gc --no-warnings "$WORK/$name.js" \
            > "$WORK/node.out" 2> "$WORK/node.err" < /dev/null || rc=$?
        if [ "$rc" -ne 0 ] || ! grep -qxF "stream-fixture: ok" "$WORK/node.out"; then
            echo "FAIL $name: node exit $rc"
            head -8 "$WORK/node.err" | sed 's/^/  stderr: /'
            status="FAIL"; FAIL=1
        fi
    fi
    echo "$status $name"
done < "$DIR/manifest.txt"

if [ -z "$ONLY" ]; then
    cat "$DIR"/corpus-*.js > "$WORK/corpus.js"
    WANT="$(cat "$DIR/corpus.digest" 2>/dev/null || true)"
    if [ "$NODE_ONLY" -eq 0 ]; then
        rc=0
        (cd "$PROJECT_DIR" && timeout "$LIMIT" "$JSSE" --node "$WORK/corpus.js") \
            > "$WORK/jsse.out" 2> "$WORK/jsse.err" || rc=$?
        got="$(tail -n 1 "$WORK/jsse.out")"
        if [ "$rc" -ne 0 ] || [ "$got" != "$WANT" ]; then
            echo "FAIL corpus: jsse exit $rc, '$got' != pinned '$WANT'"
            head -8 "$WORK/jsse.err" | sed 's/^/  stderr: /'
            FAIL=1
        else
            echo "ok   corpus: jsse matches the pinned digest ($WANT)"
        fi
    fi
    if [ "$CROSS_CHECK" -eq 1 ] || [ "$NODE_ONLY" -eq 1 ]; then
        rc=0
        (cd "$PROJECT_DIR" && timeout "$LIMIT" "$NODE_BIN" "$WORK/corpus.js") \
            > "$WORK/node.out" 2> "$WORK/node.err" || rc=$?
        if [ "$rc" -ne 0 ]; then
            echo "FAIL corpus: node exit $rc"
            head -8 "$WORK/node.err" | sed 's/^/  stderr: /'
            FAIL=1
        elif [ "$(tail -n 1 "$WORK/node.out")" != "$WANT" ]; then
            echo "FAIL corpus: node digest '$(tail -n 1 "$WORK/node.out")' != pinned '$WANT'"
            FAIL=1
        elif [ "$NODE_ONLY" -eq 0 ]; then
            if ! diff -u "$WORK/node.out" "$WORK/jsse.out" > "$WORK/diff.txt"; then
                echo "FAIL corpus: stdout differs (--- node, +++ jsse)"
                head -40 "$WORK/diff.txt"
                FAIL=1
            else
                echo "ok   corpus: jsse and node output are byte-identical"
            fi
        else
            echo "ok   corpus: node matches the pinned digest"
        fi
    fi
fi

if [ "$FAIL" -ne 0 ]; then
    echo "node-webstreams oracle: FAILED"
    exit 1
fi
echo "node-webstreams oracle: all passed"
