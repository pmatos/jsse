# Luxon — immutable date/time values built on Intl.DateTimeFormat and IANA
# zones. The pinned suite has 58 Jest files expanding to 1,152 tests.
#
# Jest's CLI needs fs/workers, so gen-luxon-entry.js creates a static bundle
# entry and installs Jest's published matcher core. The shared TAP harness is
# explicitly enabled on both engines; Node still validates the identical bundle
# and exact test count. Luxon's own test script pins America/New_York.
#
# Node: 1,152/1,152. jsse: 1,045/1,152, with the visible failures concentrated
# in the Intl/system-zone gaps tracked by jsse#262–#265.

LIB_REPO="https://github.com/moment/luxon.git"
LIB_REF="3.7.2"
LIB_ENTRY="test/jsse-entry.js"
LIB_ESBUILD_PLATFORM="browser"
LIB_SHIMS=("node-test-harness-force.js" "node-test-harness.js")
LIB_ENV=("TZ=America/New_York" "LANG=en_US.utf8")
LIB_EXPECT_COUNT="1152"
LIB_TIMEOUT="3600"

lib_prepare() {
    # Keep only the assertion package consumed by the generated in-process
    # entry. The native Jest/Babel/docs toolchain is unnecessary for esbuild.
    node -e "const p=require('./package.json'); p.devDependencies={expect:'29.7.0'}; p.scripts={}; p.sideEffects=true; require('fs').writeFileSync('package.json', JSON.stringify(p,null,2)+'\n')"
    npm install --ignore-scripts --no-audit --no-fund
    node "$SCRIPT_DIR/patch-luxon-icu.js"
    node "$SCRIPT_DIR/gen-luxon-entry.js" "$LIB_ENTRY"
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
