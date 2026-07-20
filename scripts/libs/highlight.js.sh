# highlight.js — markup and language auto-detection fixtures across all 192
# built-in grammars. The generated entry registers the source grammars and
# embeds every fixture so neither engine needs a filesystem API at runtime.

LIB_REPO="https://github.com/highlightjs/highlight.js.git"
LIB_REF="11.11.2"
LIB_ENTRY="test/jsse-entry.js"
LIB_ESBUILD_PLATFORM="neutral"
LIB_EXPECT_COUNT="731" # 536 markup + 195 auto-detection inputs (three grammars opt out)
LIB_TIMEOUT="3600"

lib_prepare() {
  # The source core imports deep-freeze-es6. The distributed package normally
  # bundles it, so install only that pinned dependency instead of the full
  # build/test toolchain.
  node -e "const p=require('./package.json'); p.devDependencies={}; p.scripts={}; require('fs').writeFileSync('package.json', JSON.stringify(p,null,2)+'\n')"
  npm install --no-save --ignore-scripts --no-audit --no-fund deep-freeze-es6@3.0.2
  node "$SCRIPT_DIR/gen-highlightjs-entry.js" "$LIB_ENTRY"
}

lib_verdict() {
  local out="$1" rc="$2" line passed failed total
  line="$(grep -oE 'HighlightJS: [0-9]+ passed, [0-9]+ failed, [0-9]+ total' "$out" | tail -1 || true)"
  if [ -z "$line" ]; then echo "FAIL 0"; return 1; fi
  if [[ "$line" =~ HighlightJS:\ ([0-9]+)\ passed,\ ([0-9]+)\ failed,\ ([0-9]+)\ total ]]; then
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
