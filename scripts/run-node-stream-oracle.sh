#!/usr/bin/env bash
# Cross-check the embedded `stream` module (issue #1233) against Node.
#
# Usage:
#   ./scripts/run-node-stream-oracle.sh [--node] [--no-cross-check]
#
# 1. Every test in scripts/stream-fixtures/manifest.txt (Node's own
#    test/parallel/test-stream-*.js, verbatim) is wrapped by harness-assert.js and
#    harness.js and must print "stream-fixture: ok" and exit 0 on jsse (`--node`,
#    via require("stream")) and on Node. Tests flagged needs-webstreams run once
#    web streams exist, and are skipped (and reported) before. Lines starting with "!" document tests that are not vendored.
# 2. scripts/stream-fixtures/corpus-{a,b,run}.js (event order, backpressure and drain,
#    pipe, pipeline, finished, compose, operators, errors) must print
#    byte-identical stdout on both, and its digest must equal
#    scripts/stream-fixtures/corpus.digest.
#
# Options:
#   --node             run on Node only (reference / regenerating corpus.digest)
#   --no-cross-check   run on jsse only (exit 0 and the pinned digest required)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DIR="$PROJECT_DIR/scripts/stream-fixtures"
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

HAVE_WEBSTREAMS=0
if [ "$NODE_ONLY" -eq 0 ]; then
    probe="$("$JSSE" --node -e 'print(typeof ReadableStream)' 2>/dev/null || true)"
    [ "$probe" = "function" ] && HAVE_WEBSTREAMS=1
fi

FAIL=0
while read -r name flags; do
    case "$name" in ""|"#"*|"!") continue ;; esac
    {
        cat "$DIR/harness-assert.js" "$DIR/harness.js"
        echo "(function (require, __filename, __dirname) {"
        cat "$DIR/$name.js"
        printf '\n})(__sRequire, "/node/test/parallel/%s.js", "/node/test/parallel");\n' "$name"
    } > "$WORK/$name.js"
    status="ok  "
    if [ "$NODE_ONLY" -eq 0 ]; then
        if [[ " $flags " == *" needs-webstreams "* ]] && [ "$HAVE_WEBSTREAMS" -eq 0 ]; then
            status="skip"
        else
            rc=0
            "$JSSE" --node "$WORK/$name.js" > "$WORK/jsse.out" 2> "$WORK/jsse.err" < /dev/null || rc=$?
            if [ "$rc" -ne 0 ] || ! grep -qxF "stream-fixture: ok" "$WORK/jsse.out"; then
                echo "FAIL $name: jsse --node exit $rc"
                head -5 "$WORK/jsse.err" | sed 's/^/  stderr: /'
                status="FAIL"; FAIL=1
            fi
        fi
    fi
    if [ "$CROSS_CHECK" -eq 1 ] || [ "$NODE_ONLY" -eq 1 ]; then
        rc=0
        "$NODE_BIN" --expose-gc --no-warnings "$WORK/$name.js" > "$WORK/node.out" 2> "$WORK/node.err" < /dev/null || rc=$?
        if [ "$rc" -ne 0 ] || ! grep -qxF "stream-fixture: ok" "$WORK/node.out"; then
            echo "FAIL $name: node exit $rc"
            head -5 "$WORK/node.err" | sed 's/^/  stderr: /'
            status="FAIL"; FAIL=1
        fi
    fi
    echo "$status $name"
done < "$DIR/manifest.txt"

cat "$DIR/corpus-a.js" "$DIR/corpus-b.js" "$DIR/corpus-run.js" > "$WORK/corpus.js"
WANT="$(cat "$DIR/corpus.digest" 2>/dev/null || true)"
if [ "$NODE_ONLY" -eq 0 ]; then
    rc=0
    (cd "$PROJECT_DIR" && "$JSSE" --node "$WORK/corpus.js") > "$WORK/jsse.out" 2> "$WORK/jsse.err" || rc=$?
    got="$(tail -n 1 "$WORK/jsse.out")"
    if [ "$rc" -ne 0 ] || [ "$got" != "$WANT" ]; then
        echo "FAIL corpus: jsse exit $rc, '$got' != pinned '$WANT'"
        head -5 "$WORK/jsse.err" | sed 's/^/  stderr: /'
        FAIL=1
    else
        echo "ok   corpus: jsse matches the pinned digest ($WANT)"
    fi
fi
if [ "$CROSS_CHECK" -eq 1 ] || [ "$NODE_ONLY" -eq 1 ]; then
    rc=0
    (cd "$PROJECT_DIR" && "$NODE_BIN" "$WORK/corpus.js") > "$WORK/node.out" 2> "$WORK/node.err" || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "FAIL corpus: node exit $rc"
        head -5 "$WORK/node.err" | sed 's/^/  stderr: /'
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

if [ "$FAIL" -ne 0 ]; then
    echo "node-stream oracle: FAILED"
    exit 1
fi
echo "node-stream oracle: all passed"
