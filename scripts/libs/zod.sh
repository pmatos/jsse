# zod — TypeScript-first schema validation, exercised in normal and jitless
# modes from the same statically bundled v4 classic runtime suite.
#
# Native Vitest at the pinned tag reports 1,092 passing runtime tests across
# 79 files (typechecking disabled). The runner executes every upstream test in
# both modes and locks the combined 2,184-case count on both engines.
# Normal and jitless execute in separate processes so the stress corpus also
# isolates heap, host-job, and async-continuation state between modes.

LIB_REPO="https://github.com/colinhacks/zod.git"
LIB_REF="v4.4.3"
LIB_ENTRY="packages/zod/jsse-entry.ts"
LIB_ESBUILD_PLATFORM="browser"
LIB_ESBUILD_EXTRA=(
    "--conditions=@zod/source"
    "--keep-names"
    "--alias:vitest=./packages/zod/jsse-vitest.ts"
    "--alias:node:crypto=./packages/zod/jsse-portability.ts"
    "--alias:node:util=./packages/zod/jsse-portability.ts"
    "--alias:recheck=./packages/zod/jsse-portability.ts"
    "--alias:@seriousme/openapi-schema-validator=./packages/zod/jsse-portability.ts"
    "--alias:@web-std/file=./packages/zod/jsse-portability.ts"
)
LIB_SHIMS=("zod-host-shim.js" "node-test-harness-force.js" "node-test-harness.js")
LIB_BUNDLE_PREFIXES=("zod-normal-mode.js" "zod-jitless-mode.js")
LIB_SEPARATE_BUNDLES=1
LIB_EXPECT_COUNT="2184"
LIB_TIMEOUT="3600"

lib_prepare() {
    # The runtime corpus needs only its matcher/serializer and a bundled URL
    # implementation. Test-only Node modules are replaced by the symmetric
    # portability seam copied below; Zod itself has no runtime dependencies.
    node -e "const p=require('./package.json'); p.devDependencies={}; p.scripts={}; p.sideEffects=true; require('fs').writeFileSync('package.json', JSON.stringify(p,null,2)+'\n')"
    node -e "const f='packages/zod/package.json'; const p=require('./'+f); p.sideEffects=true; require('fs').writeFileSync(f, JSON.stringify(p,null,2)+'\n')"
    npm install --ignore-scripts --no-audit --no-fund --no-save \
        expect@29.7.0 @vitest/pretty-format@4.1.5 whatwg-url@17.1.0

    cp "$SCRIPT_DIR/zod-vitest-shim.ts" packages/zod/jsse-vitest.ts
    cp "$SCRIPT_DIR/zod-test-portability.ts" packages/zod/jsse-portability.ts
    node "$SCRIPT_DIR/gen-zod-entry.js" "$LIB_ENTRY"
}

lib_verdict() {
    local out="$1" rc="$2" line P F T summaries=0 total=0 failed=0
    while IFS= read -r line; do
        if [[ "$line" =~ PASS:\ ([0-9]+)[[:space:]]+FAIL:\ ([0-9]+)[[:space:]]+TOTAL:\ ([0-9]+) ]]; then
            P="${BASH_REMATCH[1]}"; F="${BASH_REMATCH[2]}"; T="${BASH_REMATCH[3]}"
            summaries=$((summaries + 1))
            total=$((total + T))
            failed=$((failed + F))
        fi
    done < <(grep -oE 'PASS: [0-9]+[[:space:]]+FAIL: [0-9]+[[:space:]]+TOTAL: [0-9]+' "$out" || true)
    if [ "$rc" -eq 0 ] && [ "$summaries" -eq 2 ] && [ "$failed" -eq 0 ] && [ "$total" -gt 0 ]; then
        echo "PASS $total"; return 0
    fi
    echo "FAIL $total"; return 1
}
