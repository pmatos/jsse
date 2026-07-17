# qs — nested query-string parsing/stringifying, charset handling, prototype
# pollution guards, Buffer inputs, and Map/WeakMap-backed side channels.
#
# node-test-harness.js supplies a focused tape adapter on JSSE. The tape module
# selector exports real upstream tape on Node, so the identical bundle keeps an
# independent framework oracle and both engines must report the same assertion
# count.

LIB_REPO="https://github.com/ljharb/qs.git"
LIB_REF="v6.15.3"
LIB_ENTRY="test/jsse-entry.js"
LIB_ESBUILD_PLATFORM="node"
LIB_ESBUILD_EXTRA=(
    --alias:buffer=./test/jsse-buffer.js
    --alias:tape=./test/jsse-tape.js
    --alias:string_decoder=./test/jsse-string-decoder.js
    --alias:util=./test/jsse-util.js
)
LIB_SHIMS=("node-test-harness.js" "libs/qs-jsse-shim.js")
LIB_EXPECT_COUNT="1013"
LIB_TIMEOUT="600"

lib_prepare() {
    # Retain only the dependencies consumed by the upstream test files. The
    # lint/docs/release toolchain is unrelated to the runtime corpus.
    node -e "const p=require('./package.json'); p.devDependencies={ 'es-value-fixtures':'1.7.1', 'for-each':'0.3.5', 'has-bigints':'1.1.0', 'has-override-mistake':'1.0.1', 'has-property-descriptors':'1.0.2', 'has-proto':'1.2.0', 'has-symbols':'1.1.0', 'iconv-lite':'0.5.2', 'mock-property':'1.1.2', 'object-inspect':'1.13.4', 'safer-buffer':'2.1.2', tape:'5.10.2' }; p.scripts={}; require('fs').writeFileSync('package.json', JSON.stringify(p,null,2)+'\n')"
    npm install --ignore-scripts --no-audit --no-fund
    node "$SCRIPT_DIR/patch-qs-diagnostic.js" test/parse.js
    cp "$SCRIPT_DIR/node-buffer-module.js" test/jsse-buffer.js
    cp "$SCRIPT_DIR/node-tape-module.js" test/jsse-tape.js
    cp "$SCRIPT_DIR/node-string-decoder-module.js" test/jsse-string-decoder.js
    cp "$SCRIPT_DIR/node-util-module.js" test/jsse-util.js
    cp "$SCRIPT_DIR/libs/qs-jsse-entry.js" "$LIB_ENTRY"
}

lib_verdict() {
    local out="$1" rc="$2" tests pass fail
    tests="$(grep -oE '# tests [0-9]+' "$out" | tail -1 | awk '{print $3}' || true)"
    pass="$(grep -oE '# pass[[:space:]]+[0-9]+' "$out" | tail -1 | awk '{print $3}' || true)"
    fail="$(grep -oE '# fail[[:space:]]+[0-9]+' "$out" | tail -1 | awk '{print $3}' || true)"
    fail="${fail:-0}"
    tests="${tests:-0}"
    pass="${pass:-0}"
    if [ "$rc" -eq 0 ] && [ "$tests" -gt 0 ] && [ "$pass" -eq "$tests" ] && [ "$fail" -eq 0 ]; then
        echo "PASS $tests"
        return 0
    fi
    echo "FAIL $tests"
    return 1
}
