# decimal.js — MikeMcl arbitrary-precision Decimal type.
#
# Ships a self-contained in-process test runner (test/test.js iterates a
# module list; each module pulls in test/setup.js which defines the global `T`
# harness). No dependencies and no build step. The runner loads modules with a
# dynamic require(), which esbuild cannot bundle, so lib_prepare rewrites the
# entry into static requires (see scripts/gen-mikemcl-entry.js). Output ends
# with "In total, X of Y tests passed", which verdict_in_total parses.

LIB_REPO="https://github.com/MikeMcl/decimal.js.git"
LIB_REF="v10.6.0"
LIB_ENTRY="test/jsse-entry.js"

lib_prepare() {
    node "$SCRIPT_DIR/gen-mikemcl-entry.js" test/test.js "$LIB_ENTRY"
}

lib_verdict() { verdict_in_total "$1"; }
