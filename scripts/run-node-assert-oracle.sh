#!/usr/bin/env bash
# Cross-check the embedded `assert` module (issue #1225) against Node.
#
# Usage:
#   ./scripts/run-node-assert-oracle.sh [--node] [--no-cross-check]
#
# 1. Every test in scripts/assert-fixtures/manifest.txt (Node's own
#    test/parallel/test-assert*.js, verbatim) is wrapped by harness.js and must
#    print "assert-fixture: ok" and exit 0 on jsse (`--node`, via
#    require("assert")) and on Node. On jsse only, "@test|name|reason" lines skip
#    single node:test cases and "~test|sed-expression|reason" lines patch the
#    wrapped test text (engine gaps, each tied to a reason); lines starting with
#    "!" document tests that are not vendored.
# 2. scripts/assert-fixtures/corpus-{a,b,run}.js (the AssertionError message, operator,
#    generatedMessage and diff of a matrix of failing assertions) must print
#    byte-identical stdout on both, and its digest must equal
#    scripts/assert-fixtures/corpus.digest.
#
# Options:
#   --node             run on Node only (reference / regenerating corpus.digest)
#   --no-cross-check   run on jsse only (exit 0 and the pinned digest required)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DIR="$PROJECT_DIR/scripts/assert-fixtures"
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

export ASSERT_FIXTURE_DIR="$DIR"
FAIL=0
while read -r name sum; do
    case "$name" in ""|"#"*|"!"*|"@"*|"~"*) continue ;; esac
    JSSE_SKIP="$(grep "^@$name|" "$DIR/manifest.txt" | cut -d'|' -f2 || true)"
    if [ -f "$DIR/$name.js" ]; then
        cp "$DIR/$name.js" "$WORK/$name.src"
    else
        cat "$DIR/$name.js".part* > "$WORK/$name.src"
    fi
    if [ "$(sha256sum "$WORK/$name.src" | cut -d' ' -f1)" != "$sum" ]; then
        echo "FAIL $name: vendored text does not match sha256 $sum"
        FAIL=1
        continue
    fi
    {
        cat "$DIR/harness.js"
        echo "(function (require, __filename, __dirname) {"
        cat "$WORK/$name.src"
        printf '\n})(__aRequire, "/node/test/parallel/%s.js", "/node/test/parallel");\n__aStart();\n' "$name"
    } > "$WORK/$name.js"
    cp "$WORK/$name.js" "$WORK/$name.jsse.js"
    while IFS='|' read -r _ expr _; do
        sed -i -e "$expr" "$WORK/$name.jsse.js"
    done < <(grep "^~$name|" "$DIR/manifest.txt" || true)
    status="ok  "
    if [ "$NODE_ONLY" -eq 0 ]; then
        rc=0
        ASSERT_FIXTURE_SKIP="$JSSE_SKIP" "$JSSE" --node "$WORK/$name.jsse.js" > "$WORK/jsse.out" 2> "$WORK/jsse.err" < /dev/null || rc=$?
        if [ "$rc" -ne 0 ] || ! grep -q "^assert-fixture: ok" "$WORK/jsse.out"; then
            echo "FAIL $name: jsse --node exit $rc"
            head -20 "$WORK/jsse.out" | sed 's/^/  stdout: /'
            head -5 "$WORK/jsse.err" | sed 's/^/  stderr: /'
            status="FAIL"; FAIL=1
        fi
    fi
    if [ "$CROSS_CHECK" -eq 1 ] || [ "$NODE_ONLY" -eq 1 ]; then
        rc=0
        ASSERT_FIXTURE_SKIP= "$NODE_BIN" --expose-gc --no-warnings "$WORK/$name.js" > "$WORK/node.out" 2> "$WORK/node.err" < /dev/null || rc=$?
        if [ "$rc" -ne 0 ] || ! grep -q "^assert-fixture: ok" "$WORK/node.out"; then
            echo "FAIL $name: node exit $rc"
            head -20 "$WORK/node.out" | sed 's/^/  stdout: /'
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
    echo "node-assert oracle: FAILED"
    exit 1
fi
echo "node-assert oracle: all passed"
