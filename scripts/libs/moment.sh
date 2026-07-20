# Moment — legacy date/time parsing, formatting, manipulation, and locale
# behavior. The pinned suite contains QUnit modules for Moment's core plus all
# 138 locale test files.
#
# Upstream's Grunt/node-qunit path discovers separately transpiled test files
# through fs and loads locale definitions through dynamic require().
# gen-moment-entry.js replaces only that host-facing discovery layer with
# static imports. The shared QUnit adapter runs the unchanged suite on both
# engines, reports every failure, and separately exposes the registered test
# count for the Node cross-check.

LIB_REPO="https://github.com/moment/moment.git"
LIB_REF="2.30.1"
LIB_ENTRY="src/test/jsse-entry.js"
LIB_ESBUILD_PLATFORM="browser"
LIB_SHIMS=("node-test-harness-force.js" "node-test-harness.js")
LIB_ENV=("TZ=America/New_York" "LANG=en_US.utf8" "LC_ALL=en_US.utf8")
LIB_EXPECT_COUNT="3871" # locked: 2.30.1 registered tests, equal on JSSE and Node
LIB_TIMEOUT="3600"

lib_prepare() {
    node "$SCRIPT_DIR/patch-moment-bundle.js"
    node "$SCRIPT_DIR/gen-moment-entry.js" "$LIB_ENTRY"
}

lib_verdict() {
    local out="$1" rc="$2" summary count P F T
    summary="$(grep -oE 'PASS: [0-9]+[[:space:]]+FAIL: [0-9]+[[:space:]]+TOTAL: [0-9]+' "$out" | tail -1 || true)"
    count="$(grep -oE 'Moment: [0-9]+ tests' "$out" | tail -1 | grep -oE '[0-9]+' || true)"
    if [ -z "$summary" ] || [ -z "$count" ]; then echo "FAIL 0"; return 1; fi
    if [[ "$summary" =~ PASS:\ ([0-9]+)[[:space:]]+FAIL:\ ([0-9]+)[[:space:]]+TOTAL:\ ([0-9]+) ]]; then
        P="${BASH_REMATCH[1]}"; F="${BASH_REMATCH[2]}"; T="${BASH_REMATCH[3]}"
        if [ "$rc" -eq 0 ] && [ "$F" -eq 0 ] && [ "$T" -gt 0 ]; then echo "PASS $count"; return 0; fi
        echo "FAIL $count"; return 1
    fi
    echo "FAIL 0"; return 1
}
