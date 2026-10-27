#!/usr/bin/env bash
# Cross-check the embedded `events` module, EventTarget and AbortSignal
# (issue #1223) against Node.
#
# Usage:
#   ./scripts/run-node-events-oracle.sh [--node] [--no-cross-check]
#
# 1. Every test in scripts/events-fixtures/manifest.txt (Node's own
#    test/parallel files, verbatim) is wrapped by scripts/events-fixtures/harness.js
#    (.mjs tests: their common and node:test imports point at esm-shim.mjs) and
#    must print "events-fixture: ok" and exit 0 on jsse (`--node`) and on Node.
#    A test whose "// Flags:" line names --expose-internals runs with that flag
#    on both (the jsse switch is --node only).
#    Subtests jsse cannot run are skipped by name in harness.js; manifest lines
#    starting with "!" record the Node tests that are not vendored, and why.
# 2. scripts/events-fixtures/corpus-emitter.js followed by corpus-web.js (events,
#    EventTarget, AbortSignal.any/timeout, errors) must print byte-identical
#    stdout on both, and its digest must equal scripts/events-fixtures/corpus.digest.
#
# Options:
#   --node             run on Node only (reference / regenerating corpus.digest)
#   --no-cross-check   run on jsse only (exit 0 and the pinned digest required)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DIR="$PROJECT_DIR/scripts/events-fixtures"
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

FAIL=0
while read -r name sum; do
    case "$name" in ""|"#"*|"!"*|"~"*) continue ;; esac
    src="$DIR/$name.js"
    if [[ "$name" == *.mjs ]]; then
        src="$DIR/$name"
    elif [ ! -f "$src" ]; then
        src="$WORK/$name.src"
        cat $(ls "$DIR/$name".js.part* | sort -V) > "$src"
        if [ "$(sha256sum "$src" | cut -d' ' -f1)" != "$sum" ]; then
            echo "FAIL $name: vendored text does not match sha256 $sum"
            FAIL=1
            continue
        fi
    fi
    flags=()
    if grep -qE '^// Flags:.*--expose-internals' "$src"; then
        flags=(--expose-internals)
    fi
    if [[ "$name" == *.mjs ]]; then
        file="$WORK/$name"
        cp "$DIR/esm-shim.mjs" "$WORK/esm-shim.mjs"
        sed -E -e "s#(['\"])../common/index.mjs\1#\1./esm-shim.mjs\1#" -e "s#(['\"])node:test\1#\1./esm-shim.mjs\1#" \
            "$DIR/$name" > "$file"
    else
        file="$WORK/$name.js"
        {
            cat "$DIR/harness.js"
            echo "try { (function (require, __filename, __dirname) {"
            cat "$src"
            printf '\n})(__eventsRequire, "/node/test/parallel/%s.js", "/node/test/parallel"); } finally { __eventsFinish(); }\n' "$name"
        } > "$file"
    fi
    jfile="$file"
    if grep -q "^~$name|" "$DIR/manifest.txt"; then
        jfile="$WORK/$name.jsse.js"
        cp "$file" "$jfile"
        while IFS='|' read -r _ expr _; do
            python3 -I - "$expr" "$jfile" <<'PY'
import sys
expr, path = sys.argv[1:]
frm, to = expr[2:-1].split("#", 1)
text = open(path, encoding="utf-8").read()
if frm not in text:
    sys.exit("patch does not apply: " + frm)
open(path, "w", encoding="utf-8").write(text.replace(frm, to))
PY
        done < <(grep "^~$name|" "$DIR/manifest.txt")
    fi
    status="ok  "
    if [ "$NODE_ONLY" -eq 0 ]; then
        rc=0
        "$JSSE" --node ${flags[@]+"${flags[@]}"} "$jfile" > "$WORK/jsse.out" 2> "$WORK/jsse.err" < /dev/null || rc=$?
        if [ "$rc" -ne 0 ] || ! grep -qxF "events-fixture: ok" "$WORK/jsse.out"; then
            echo "FAIL $name: jsse --node exit $rc"
            head -5 "$WORK/jsse.err" | sed 's/^/  stderr: /'
            status="FAIL"; FAIL=1
        fi
    fi
    if [ "$CROSS_CHECK" -eq 1 ] || [ "$NODE_ONLY" -eq 1 ]; then
        rc=0
        "$NODE_BIN" --expose-gc --no-warnings ${flags[@]+"${flags[@]}"} "$file" > "$WORK/node.out" 2> "$WORK/node.err" < /dev/null || rc=$?
        if [ "$rc" -ne 0 ] || ! grep -qxF "events-fixture: ok" "$WORK/node.out"; then
            echo "FAIL $name: node exit $rc"
            head -5 "$WORK/node.err" | sed 's/^/  stderr: /'
            status="FAIL"; FAIL=1
        fi
    fi
    echo "$status $name"
done < "$DIR/manifest.txt"

cat "$DIR/corpus-emitter.js" "$DIR/corpus-web.js" > "$WORK/corpus.js"
WANT="$(cat "$DIR/corpus.digest")"
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
    (cd "$PROJECT_DIR" && "$NODE_BIN" --expose-gc "$WORK/corpus.js") > "$WORK/node.out" 2> "$WORK/node.err" || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "FAIL corpus: node exit $rc"
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
    echo "node-events oracle: FAILED"
    exit 1
fi
echo "node-events oracle: all passed"
