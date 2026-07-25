# uuid — RFC 9562 UUID generation/parsing (v1/v3/v4/v5/v6/v7), exercising
# TypedArrays, bitwise ops, hex parsing, and a new randomness host seam.
#
# Pinned to the browser build (dist/, not dist-node/): v3/v5 then exercise
# uuid's own pure-JS MD5/SHA-1 (src/md5-browser.ts, src/sha1-browser.ts)
# instead of node:crypto's createHash, and v1/v4/v6/v7 draw randomness from
# crypto.getRandomValues, backed by node-crypto-shim.js's __host_random_bytes
# seam (#229) rather than a node:crypto module. Upstream's own test files
# (src/test/*.test.ts, compiled by tsc) import "node:test" and
# "node:assert/strict" directly and are used unmodified; uuid-jsse-require-shim.js
# resolves those two specifiers on jsse only, so Node keeps running the exact
# same suite against its own native node:test/assert (see that file's header
# for the mechanism).

LIB_REPO="https://github.com/uuidjs/uuid.git"
LIB_REF="v14.0.1"
LIB_ENTRY="dist/uuid-jsse-entry.js"
LIB_SHIMS=("node-crypto-shim.js" "node-test-harness.js" "libs/uuid-jsse-require-shim.js")
LIB_EXPECT_COUNT="75"   # locked: v14.0.1 upstream node:test suite, equal on jsse and Node
LIB_TIMEOUT="120"

lib_prepare() {
    # Keep preparation small and deterministic: the corpus only needs the
    # TypeScript compiler (lint/docs/release/browser-test tooling is unrelated
    # to the runtime suite).
    # sideEffects:true — otherwise esbuild tree-shakes away every bare
    # `import "./test/*.test.js"` (they register describe/test calls but
    # export nothing), and the whole bundle collapses to nothing.
    node -e "const p=require('./package.json'); p.devDependencies={typescript:p.devDependencies.typescript}; p.scripts={}; p.sideEffects=true; require('fs').writeFileSync('package.json', JSON.stringify(p,null,2)+'\n')"
    npm install --ignore-scripts --no-audit --no-fund
    # tsc's NodeNext module resolution needs @types/node in scope to type the
    # ambient node:test/node:assert/Buffer/process references in src/ — it was
    # only ever present transitively through the lint/release devDependencies
    # trimmed away above. Vendor a pinned copy of the real "assert" npm
    # package too (see uuid-assert-connector.js for why). Both --no-save
    # installs must land in one npm invocation: a second, separate `npm
    # install --no-save` call reconciles the whole tree against package.json
    # and prunes whatever the first call added.
    npm install --ignore-scripts --no-audit --no-fund --no-save @types/node@22 assert@2.1.0

    npx tsc -p tsconfig.json

    # Swap in the browser-flavoured md5/sha1/rng (pure-JS hashes +
    # crypto.getRandomValues) in place of the node:crypto-backed Node build,
    # matching scripts/build.sh's own browser/node split.
    (cd dist && for f in *-browser*; do mv "$f" "${f/-browser/}"; done)

    cp "$SCRIPT_DIR/libs/uuid-assert-connector.js" dist/uuid-assert-connector.js
    cp "$SCRIPT_DIR/libs/uuid-jsse-entry.js" dist/uuid-jsse-entry.js
}

# Both engines emit a node:test-shaped tests/pass/fail summary: jsse via
# node-test-harness.js's own TAP runner (always "# " prefixed), Node's real
# node:test via whichever reporter is its default for the local Node version
# — the TAP reporter uses "# ", the newer default "spec" reporter uses "ℹ "
# (Node 20+). Match either prefix rather than pin one reporter's output shape.
lib_verdict() {
    local out="$1" rc="$2" tests pass fail
    tests="$(grep -oE '^(#|ℹ) tests [0-9]+' "$out" | tail -1 | awk '{print $3}' || true)"
    pass="$(grep -oE '^(#|ℹ) pass [0-9]+' "$out" | tail -1 | awk '{print $3}' || true)"
    fail="$(grep -oE '^(#|ℹ) fail [0-9]+' "$out" | tail -1 | awk '{print $3}' || true)"
    tests="${tests:-0}"
    pass="${pass:-0}"
    fail="${fail:-0}"
    if [ "$rc" -eq 0 ] && [ "$tests" -gt 0 ] && [ "$fail" -eq 0 ] && [ "$pass" -eq "$tests" ]; then
        echo "PASS $tests"
        return 0
    fi
    echo "FAIL $tests"
    return 1
}
