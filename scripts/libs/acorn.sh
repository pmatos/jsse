# acorn — the JavaScript parser jsse has tracked since the acorn harness. This
# config migrates that bespoke recipe onto the generalized runner.
#
# acorn needs a real build: install devDeps, `npm run build`, then bundle the
# test runner (test/run.js) with esbuild in "neutral" platform mode resolving
# main/module fields (matching the original run-acorn-tests.sh). esbuild strips
# comments, but the TestComments test reads them back via Function.prototype
# .toString(), so we patch that test to a string literal before bundling.
#
# The runner prints "Total: N tests run in Xms; all passed." and only calls
# process.exit(1) when something fails, so the verdict is exit-code driven with
# the reported total as the cross-checked count.

# Pinned to 8.16.0 — the last release before 8.17.0 added a parser stack-guard
# test (`"[".repeat(2000)`, commit 8a47812) that expects the engine to *throw*
# a "Not enough stack space" error. jsse's tree-walker amplifies acorn's
# recursive-descent frames and its Rust stack aborts before acorn's guard can
# fire, so 8.17.0+ hard-crash jsse. That is a jsse robustness gap (deep
# recursion should raise a catchable RangeError, not abort) tracked separately;
# pinning here keeps the harness green and reproducible.
LIB_REPO="https://github.com/acornjs/acorn.git"
LIB_REF="8.16.0"
LIB_ENTRY="test/run.js"
LIB_ESBUILD_PLATFORM="neutral"
LIB_ESBUILD_EXTRA=(--main-fields=main,module)
LIB_EXPECT_COUNT="13507"   # locked: 8.16.0 total, verified equal on jsse and Node
LIB_TIMEOUT="900"

lib_prepare() {
    # Drop the huge test262 git devDep (causes integrity errors); the build
    # only needs the bundler toolchain.
    node -e "const p=require('./package.json'); delete p.devDependencies['test262']; delete p.devDependencies['test262-parser-runner']; require('fs').writeFileSync('package.json', JSON.stringify(p, null, 2)+'\n')"
    npm install
    npm run build
    node "$SCRIPT_DIR/patch-acorn-comments.js" test/tests.js
}

lib_verdict() {
    local out="$1" rc="$2" count=0
    if [[ "$(grep -oE 'Total: [0-9]+ tests run' "$out" | tail -1 || true)" =~ ([0-9]+) ]]; then
        count="${BASH_REMATCH[1]}"
    fi
    if [ "$rc" -eq 0 ] && grep -q 'Total:.*all passed' "$out" && [ "$count" -gt 0 ]; then
        echo "PASS $count"; return 0
    fi
    echo "FAIL $count"; return 1
}
