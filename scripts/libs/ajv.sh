# ajv — JSON Schema validator and a sustained runtime-code-generation workload.
#
# This slice runs AJV's upstream Mocha/Chai JSON-Schema-Test-Suite integration
# (drafts 6, 7, 2019-09, and 2020-12). AJV's scripts/jsontests converts the
# pinned submodule's JSON files to static require() lists; esbuild then inlines
# the complete corpus into one bundle, with no runtime filesystem dependency.
#
# node-test-harness.js supplies Mocha-shaped globals on jsse. The generated
# entry starts real Mocha on Node, so the final registered-test count remains an
# independent same-bundle cross-check. AJV's standalone validator wrapper emits
# runtime `require("ajv/dist/runtime/*")` calls that cannot resolve from a
# single external bundle, so both engines run the four normal AJV option
# variants—roughly 22,000 generated-validator executions across 5,480 fixtures.

LIB_REPO="https://github.com/ajv-validator/ajv.git"
LIB_REF="v8.17.1"
LIB_ENTRY="spec/jsse-entry.js"
LIB_ESBUILD_PLATFORM="node"
LIB_SHIM="node-test-harness.js"
LIB_EXPECT_COUNT="5480"
LIB_TIMEOUT="2400"

lib_prepare() {
    git submodule update --init --depth 1 spec/JSON-Schema-Test-Suite

    # Install only what the validation corpus and build need. AJV's complete
    # development tree includes browser/Karma/image/native-RE2 tooling that is
    # unrelated to this in-process slice.
    node -e "const p=require('./package.json'); p.devDependencies={}; p.scripts={}; require('fs').writeFileSync('package.json', JSON.stringify(p,null,2)+'\n')"
    npm install --ignore-scripts --no-audit --no-fund --no-save \
        typescript@5.3.3 mocha@10.3.0 chai@4.4.1 ajv-formats@3.0.1 \
        json-schema-test@2.0.0 glob@10.3.10 \
        @ajv-validator/config@0.5.0 @types/node@20.11.30 \
        @types/require-from-string@1.2.3 re2@1.20.9

    node scripts/jsontests
    ./node_modules/.bin/tsc --skipLibCheck
    cp -r lib/refs dist
    rm -f \
        dist/refs/json-schema-2019-09/index.ts \
        dist/refs/json-schema-2020-12/index.ts \
        dist/refs/jtd-schema.ts
    node "$SCRIPT_DIR/patch-ajv-jsse.js" "$PWD"
    node "$SCRIPT_DIR/gen-ajv-entry.js" spec/jsse-entry.js
}

lib_verdict() {
    local out="$1" line P F T
    line="$(grep -oE 'PASS: [0-9]+[[:space:]]+FAIL: [0-9]+[[:space:]]+TOTAL: [0-9]+' "$out" | tail -1 || true)"
    if [ -z "$line" ]; then echo "FAIL 0"; return 1; fi
    if [[ "$line" =~ PASS:\ ([0-9]+)[[:space:]]+FAIL:\ ([0-9]+)[[:space:]]+TOTAL:\ ([0-9]+) ]]; then
        P="${BASH_REMATCH[1]}"; F="${BASH_REMATCH[2]}"; T="${BASH_REMATCH[3]}"
        if [ "$F" -eq 0 ] && [ "$T" -gt 0 ]; then echo "PASS $T"; return 0; fi
        echo "FAIL $T"; return 1
    fi
    echo "FAIL 0"; return 1
}
