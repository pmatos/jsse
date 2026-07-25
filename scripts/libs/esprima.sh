# esprima — second-parser cross-check for the engine (follow-up from #233,
# the esprima slice of epic #227). Zero runtime deps; ~1,650 fixture-based
# unit tests plus a generated test262 grammar-conformance corpus.
#
# Pinned to the exact unreleased commit the issue specifies (never released as
# 4.0.1): its dist/ is gitignored, so lib_prepare compiles it for real via
# esprima's own `npm run compile` (tsc + webpack@1 + a bundle fixup), which the
# "prepublish" lifecycle script runs automatically on `npm install`.
#
# api-tests.js is mocha-shaped (global describe/it, `require('assert')`) with
# no CommonJS fallback when those globals are absent, so — like Luxon — this
# forces the shared TAP harness on BOTH engines rather than leaving it inert on
# Node (see node-test-harness-force.js). `require('assert')` is aliased to the
# new scripts/node-assert-module.js selector (Node's real module on Node, a
# small polyfill on jsse). See scripts/libs/esprima-jsse-entry.js for how the
# suites are wired together, and why test/regression-tests.js isn't ported
# (minutes of jsse tree-walker time to parse a few real-world libraries
# through an interpreted parser, for 7 assertions the unit fixtures and the
# test262 corpus already dwarf in density).
LIB_REPO="https://github.com/jquery/esprima.git"
LIB_REF="512cd66c6ffd6083144b0150f09670e426252776"
LIB_ENTRY="test/jsse-entry.js"
LIB_ESBUILD_PLATFORM="node"
LIB_ESBUILD_EXTRA=(--alias:assert=./test/jsse-assert.js)
LIB_SHIMS=("node-test-harness-force.js" "node-test-harness.js")
LIB_TIMEOUT="7200"

lib_prepare() {
    npm install --no-audit --no-fund
    npm run generate-fixtures
    npm install --no-save --no-audit --no-fund everything.js@1.0.3 test262-stream@1.3.0
    node "$SCRIPT_DIR/gen-esprima-fixtures.js"
    node "$SCRIPT_DIR/gen-esprima-test262-corpus.js"
    cp "$SCRIPT_DIR/node-assert-module.js" test/jsse-assert.js
    cp "$SCRIPT_DIR/libs/esprima-jsse-entry.js" "$LIB_ENTRY"
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
