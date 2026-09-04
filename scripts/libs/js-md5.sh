# js-md5 — deterministic MD5 and HMAC-MD5 vectors over strings, Arrays,
# Buffer, TypedArrays, and ArrayBuffers.
#
# The upstream Node entry repeats the same vector files under several module
# loader configurations using require.cache eviction and adds Worker tests.
# esbuild bundles each module once and jsse has no Worker host surface, so
# lib_prepare installs a focused entry that runs both cryptographic vector files
# once. On jsse, node-test-harness.js supplies the Mocha-shaped runner; on Node,
# the entry runs real Mocha and emits the same PASS/FAIL/TOTAL summary.

LIB_REPO="https://github.com/emn178/js-md5.git"
LIB_REF="v0.8.3"
LIB_ENTRY="tests/jsse-entry.js"
LIB_ESBUILD_PLATFORM="node"
LIB_SHIM="node-test-harness.js"
LIB_EXPECT_COUNT="550"   # locked: v0.8.3 vectors, equal on jsse and Node
LIB_TIMEOUT="300"

lib_prepare() {
    # Keep preparation small and deterministic: the coverage, documentation,
    # AMD, and Worker dependencies from upstream are not used by this bundle.
    node -e "const p=require('./package.json'); p.devDependencies={}; p.scripts={}; require('fs').writeFileSync('package.json', JSON.stringify(p,null,2)+'\n')"
    npm install --no-save --no-audit --no-fund mocha@10.8.2
    cp "$SCRIPT_DIR/libs/js-md5-jsse-entry.js" "$LIB_ENTRY"
}

lib_verdict() {
    local out="$1" rc="$2" line P F T
    line="$(grep -oE 'PASS: [0-9]+[[:space:]]+FAIL: [0-9]+[[:space:]]+TOTAL: [0-9]+' "$out" | tail -1 || true)"
    if [ -z "$line" ]; then echo "FAIL 0"; return 1; fi
    if [[ "$line" =~ PASS:\ ([0-9]+)[[:space:]]+FAIL:\ ([0-9]+)[[:space:]]+TOTAL:\ ([0-9]+) ]]; then
        P="${BASH_REMATCH[1]}"; F="${BASH_REMATCH[2]}"; T="${BASH_REMATCH[3]}"
        if [ "$rc" -eq 0 ] && [ "$F" -eq 0 ] && [ "$T" -gt 0 ]; then echo "PASS $T"; return 0; fi
        echo "FAIL $T"; return 1
    fi
    echo "FAIL 0"; return 1
}
