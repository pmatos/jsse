#!/usr/bin/env bash
# Cross-check the embedded url, querystring and punycode modules and the URL /
# URLSearchParams globals (issue #1230) against Node.
#
# Usage:
#   ./scripts/run-node-url-oracle.sh [--node] [--no-cross-check]
#
# scripts/url-fixtures/corpus-*.js, concatenated in name order, is a
# deterministic ~29k-line differential corpus (WHATWG parse and every setter,
# URLSearchParams, legacy url.parse/format/resolve, file URL conversion on
# posix and win32, IDNA, querystring, punycode). It must print byte-identical
# stdout and stderr on Node and on `jsse --node`, and its final digest line must
# equal scripts/url-fixtures/corpus.digest.
#
# Options:
#   --node             run on Node only (reference / regenerating corpus.digest)
#   --no-cross-check   run on jsse only (exit 0 and the pinned digest required)

set -euo pipefail
export LC_ALL=C

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DIR="$PROJECT_DIR/scripts/url-fixtures"
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

cat "$DIR"/corpus-*.js > "$WORK/corpus.js"
WANT="$(cat "$DIR/corpus.digest")"
FAIL=0

if [ "$NODE_ONLY" -eq 0 ]; then
    rc=0
    "$JSSE" --node "$WORK/corpus.js" > "$WORK/jsse.out" 2> "$WORK/jsse.err" || rc=$?
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
    "$NODE_BIN" "$WORK/corpus.js" > "$WORK/node.out" 2> "$WORK/node.err" || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "FAIL corpus: node exit $rc"
        FAIL=1
    elif [ "$(tail -n 1 "$WORK/node.out")" != "$WANT" ]; then
        echo "FAIL corpus: node digest '$(tail -n 1 "$WORK/node.out")' != pinned '$WANT'"
        FAIL=1
    elif [ "$NODE_ONLY" -eq 0 ]; then
        for stream in out err; do
            if ! diff -u "$WORK/node.$stream" "$WORK/jsse.$stream" > "$WORK/diff.txt"; then
                echo "FAIL corpus: std$stream differs (--- node, +++ jsse)"
                head -40 "$WORK/diff.txt"
                FAIL=1
            fi
        done
        [ "$FAIL" -ne 0 ] || echo "ok   corpus: jsse and node output are byte-identical"
    else
        echo "ok   corpus: node matches the pinned digest"
    fi
fi

if [ "$FAIL" -ne 0 ]; then
    echo "node-url oracle: FAILED"
    exit 1
fi
echo "node-url oracle: all passed"
