# bignumber.js — MikeMcl arbitrary-precision decimal and non-decimal numbers.
#
# Same self-contained pattern: test/test.js iterates a method list and
# dynamic-requires each (global harness `Test` in test/tester.js). No deps, no
# build. lib_prepare rewrites the dynamic-require entry into static requires.
# (The original test/test.js reports timing via process.hrtime, which the
# generated entry drops; node-shim.js still provides hrtime for any module
# that uses it.) Output ends with "In total, X of Y tests passed".
#
# BLOCKED on a jsse engine bug this suite surfaced: in a strict-mode
# constructor, `return <call>` whose call returns a non-object makes `new`
# return that value instead of `this`. bignumber.js's constructor does
# `return parseNumeric(...)` (parseNumeric has no return → undefined), so
# `new BigNumber("Infinity"|"NaN")` yields undefined on jsse (object on Node).
# Config is correct and green on Node today; it will go green on jsse once the
# engine bug is fixed. Tracked in jsse#238.

LIB_REPO="https://github.com/MikeMcl/bignumber.js.git"
LIB_REF="v9.1.2"
LIB_ENTRY="test/jsse-entry.js"

lib_prepare() {
    node "$SCRIPT_DIR/gen-mikemcl-entry.js" test/test.js "$LIB_ENTRY"
}

lib_verdict() { verdict_in_total "$1"; }
