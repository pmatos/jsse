# UglifyJS — parser, compressor/tree-transformer, mangler, and code generator.
#
# The upstream compress runner discovers its DSL fixtures through fs and uses
# child processes for batching. The prepare-time generator embeds UglifyJS's
# own implementation and every fixture into one synchronous, filesystem-free
# entry. All 4,233 cases retain their exact transformed-output checks; the
# Node-vm/subprocess expect_stdout layer is outside this transformation slice.

LIB_REPO="https://github.com/mishoo/UglifyJS.git"
LIB_REF="v3.19.3"
LIB_ENTRY="test/jsse-entry.js"
LIB_ESBUILD_PLATFORM="neutral"
LIB_EXPECT_COUNT="4233"
LIB_TIMEOUT="14400"

lib_prepare() {
    node "$SCRIPT_DIR/gen-uglify-js-entry.js" "$LIB_ENTRY"
}

lib_verdict() {
    local out="$1" rc="$2" line passed failed total
    line="$(grep -oE 'UglifyJS compress: [0-9]+ passed, [0-9]+ failed, [0-9]+ total' "$out" | tail -1 || true)"
    if [ -z "$line" ]; then echo "FAIL 0"; return 1; fi
    if [[ "$line" =~ UglifyJS\ compress:\ ([0-9]+)\ passed,\ ([0-9]+)\ failed,\ ([0-9]+)\ total ]]; then
        passed="${BASH_REMATCH[1]}"
        failed="${BASH_REMATCH[2]}"
        total="${BASH_REMATCH[3]}"
        if [ "$rc" -eq 0 ] && [ "$failed" -eq 0 ] && [ "$passed" -eq "$total" ] && [ "$total" -gt 0 ]; then
            echo "PASS $total"; return 0
        fi
        echo "FAIL $total"; return 1
    fi
    echo "FAIL 0"; return 1
}
