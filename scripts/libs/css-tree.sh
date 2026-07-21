# css-tree — CSS tokenizer/parser/generator/walker/lexer, a new grammar
# domain distinct from the JS-focused parser/transform cluster (#233).
#
# Upstream's own suite is `mocha lib/__tests`, using only Node's built-in
# `assert` and fs.readFileSync/readdirSync/statSync to load its JSON fixture
# tree. gen-css-tree-entry.js snapshots that read-only tree (plus
# package.json) into a manifest and statically imports every top-level test
# file, replacing only the fs-discovery/require layer — the test bodies are
# unmodified upstream code. node-fs-module.js, node-path-module.js, and
# node-assert-module.js serve JSSE from that manifest / a minimal
# implementation; Node keeps its native modules, an independent oracle for
# the same generated bundle. The library's own source uses
# createRequire(import.meta.url) to load JSON (version + mdn-data tables);
# patch-css-tree-esm.js rewrites those three call sites to static ESM
# imports so esbuild can bundle them (same JSON, different loading
# mechanism — esbuild cannot inline a runtime createRequire() call, and JSSE
# has no module system to satisfy it at runtime).
#
# The shared TAP harness is force-enabled on both engines (as luxon/moment
# do), since Mocha's own CLI needs fs to discover files. helpers/setup.js is
# excluded — it only installs an Object.prototype poison-pill getter that no
# test depends on (confirmed the only __proto_pollute__ reference); the
# registered/passing counts are identical without it.
#
# jsse: 16,725/16,727 (cross-checked count against Node, which is 16,727/16,727
# green). The 2 residual failures (List#some/#filter "basic") are a genuine
# jsse engine bug, not a harness gap: a class method with a default 2nd
# parameter that calls fn.call(...) breaks on its *second* invocation after
# its *first* invocation was passed a native function (e.g. Boolean) as fn —
# tracked in jsse#355, minimal repro included there.

LIB_REPO="https://github.com/csstree/csstree.git"
LIB_REF="v3.2.1"
LIB_ENTRY="lib/__tests/jsse-entry.js"
# "node" (not "browser"): each aliased shim's real-Node fallback branch does
# require("node:fs")/require("node:path")/require("node:assert"), which only
# the "node" platform leaves unresolved-but-external at build time instead of
# hard-erroring — that branch only ever executes under the Node oracle run.
LIB_ESBUILD_PLATFORM="node"
LIB_ESBUILD_EXTRA=(
    --alias:fs=./jsse-fs.cjs
    --alias:path=./jsse-path.cjs
    --alias:assert=./jsse-assert.cjs
)
LIB_SHIMS=("node-test-harness-force.js" "node-test-harness.js")
LIB_EXPECT_COUNT="16727"
LIB_TIMEOUT="600"

lib_prepare() {
    # package.json declares "sideEffects": false, which tells esbuild that
    # bare side-effect-only imports (every generated `import "./foo.js"` —
    # each test file's whole job is calling describe/it, not exporting
    # bindings) are dead and can be dropped. Same fix as luxon.sh.
    node -e "const p=require('./package.json'); p.sideEffects=true; require('fs').writeFileSync('package.json', JSON.stringify(p,null,2)+'\n')"
    npm install --ignore-scripts --no-audit --no-fund
    node "$SCRIPT_DIR/patch-css-tree-esm.js"
    # Copied to the repo root, not lib/__tests/, so gen-css-tree-entry.js's
    # glob over lib/__tests/*.js (every top-level upstream test file) doesn't
    # sweep these up as if they were test files to import. .cjs (not .js):
    # css-tree's package.json sets "type": "module", so a plain .js file
    # here would be treated as ESM and its module.exports assignment would
    # not satisfy a default import.
    cp "$SCRIPT_DIR/node-fs-module.js" jsse-fs.cjs
    cp "$SCRIPT_DIR/node-path-module.js" jsse-path.cjs
    cp "$SCRIPT_DIR/node-assert-module.js" jsse-assert.cjs
    node "$SCRIPT_DIR/gen-css-tree-entry.js" "$LIB_ENTRY"
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
