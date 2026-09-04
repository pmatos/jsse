# prismjs — syntax-highlighting grammars exercised through the upstream
# token-stream fixtures. The prepare step embeds every non-HTML .test fixture,
# Prism core, and the dependency-ordered language component sources into one
# filesystem-free entry. Each fixture still gets a fresh Prism instance, as it
# does in Prism's own runner.

LIB_REPO="https://github.com/PrismJS/prism.git"
LIB_REF="v1.30.0"
LIB_ENTRY="tests/jsse-entry.js"
LIB_ESBUILD_PLATFORM="neutral"
LIB_EXPECT_COUNT="2563" # v1.30.0 token-stream fixtures (11 HTML fixtures excluded)
LIB_TIMEOUT="3600"

lib_prepare() {
	node "$SCRIPT_DIR/gen-prism-entry.js" "$LIB_ENTRY"
}

lib_verdict() {
	local out="$1" rc="$2" line passed failed total
	line="$(grep -oE 'PrismJS: [0-9]+ passed, [0-9]+ failed, [0-9]+ total' "$out" | tail -1 || true)"
	if [ -z "$line" ]; then echo "FAIL 0"; return 1; fi
	if [[ "$line" =~ PrismJS:\ ([0-9]+)\ passed,\ ([0-9]+)\ failed,\ ([0-9]+)\ total ]]; then
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
