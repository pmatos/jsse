# lodash — the utility library, and the proof target for the shared in-process
# test-runner harness (issue #232). Its test suite (test/test.js, ~5k tests
# expanding to 6,794 assertions) is written against QUnit and loaded via
# `root.QUnit || require('qunit-extras')`.
#
# On jsse the node-test-harness.js prelude installs a global `QUnit` adapter, so
# that probe uses our in-process adapter and the bundled qunit-extras stays
# dormant (qunit-extras needs setInterval, which jsse does not provide). On Node
# the prelude is inert, so real qunit-extras runs as the reference oracle — the
# TOTAL assertion count our adapter reports on jsse is cross-checked against the
# TOTAL real QUnit reports on Node.
#
# test.js loads the module under test and its `ui` metadata with dynamic
# require() calls esbuild cannot bundle; scripts/libs/lodash-jsse-entry.js is a
# small entry that pre-sets `root._` and `root.ui` so those branches
# short-circuit (see that file). The PhantomJS-only `webpage`/`system` modules
# are referenced in dead code paths, so they are marked external.

LIB_REPO="https://github.com/lodash/lodash.git"
LIB_REF="4.17.21"
LIB_ENTRY="test/jsse-entry.js"
LIB_ESBUILD_PLATFORM="node"
LIB_ESBUILD_EXTRA=(--external:webpage --external:system)
LIB_SHIM="node-test-harness.js"
LIB_EXPECT_COUNT="6794"   # locked: 4.17.21 total assertions, equal on jsse and Node
LIB_TIMEOUT="2400"        # tree-walker headroom (Node runs it in ~12s)

lib_prepare() {
    # Drop lodash's heavy devDependency tree (webpack/jquery/jscs/…); we only
    # need the QUnit stack for the Node oracle path plus a stable lodash for the
    # suite's `lodashStable`. Pin them for reproducibility.
    node -e "const p=require('./package.json'); p.devDependencies={}; p.scripts={}; require('fs').writeFileSync('package.json', JSON.stringify(p,null,2)+'\n')"
    npm install --no-save --no-audit --no-fund \
        qunitjs@2.4.1 qunit-extras@3.0.0 lodash@4.17.20
    # Make the suite's require-only reload blocks take lodash's own skip path on
    # jsse (Node still runs them for real — see patch-lodash-jsse.js).
    node "$SCRIPT_DIR/patch-lodash-jsse.js" test/test.js
    cp "$SCRIPT_DIR/libs/lodash-jsse-entry.js" test/jsse-entry.js
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
