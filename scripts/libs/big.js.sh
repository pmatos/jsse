# big.js — MikeMcl small-and-fast arbitrary-precision decimals.
#
# Same self-contained pattern as decimal.js: test/runner.js iterates a method
# list and dynamic-requires each (global harness `test` in test/test.js). No
# deps, no build. lib_prepare rewrites the dynamic-require entry into static
# requires; output ends with "In total, X of Y tests passed".

LIB_REPO="https://github.com/MikeMcl/big.js.git"
LIB_REF="v6.2.2"
LIB_ENTRY="test/jsse-entry.js"

lib_prepare() {
    node "$SCRIPT_DIR/gen-mikemcl-entry.js" test/runner.js "$LIB_ENTRY"
}

lib_verdict() { verdict_in_total "$1"; }
