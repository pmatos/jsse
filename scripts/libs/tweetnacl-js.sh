# tweetnacl-js — deterministic KAT vectors (hash, onetimeauth, sign) plus
# curve25519/Ed25519 Float64Array field-math random-vector suites; only the
# `.quick` files need real entropy (nacl.randomBytes/box.keyPair/sign.keyPair).
#
# Mirrors upstream's `test-node` npm script (`tape test/*.js`), i.e. the 13
# files directly under test/ — test/c/*.js needs a native addon built via its
# own Makefile and is out of scope, matching the project's existing
# no-native-addon precedent (js-sha256 skips its worker smoke test the same
# way).
#
# Every test file's first line reads `nacl` via
# `(typeof window !== 'undefined') ? window.nacl : require('../' +
# (process.env.NACL_SRC || 'nacl.min.js'))`. esbuild cannot bundle that
# require: the argument is a runtime string concatenation, and esbuild treats
# a `'../' + x` require as a directory glob-import, trying (and failing) to
# bundle every file under the repo root including .git/. lib_prepare rewrites
# that one line (identical across all 13 files) to `window.nacl`; the jsse
# entry sets `window` and preloads `nacl` first so this is a pure no-op
# simplification, not a behavior change (this is the same browser/webpack mode
# upstream's own Node entry uses, and the one js-sha256-jsse-entry.js also
# forces).
#
# nacl.min.js's own PRNG auto-init prefers the browser path
# (`self.crypto.getRandomValues`) before falling back to Node's `require('crypto')`.
# The jsse entry sets `self`, and node-crypto-shim.js (the Web Crypto shim
# already wired for the uuid harness, backed by the #229 __host_random_bytes
# syscall floor) supplies `self.crypto`, so nacl's own auto-init configures
# its PRNG with no jsse-specific code here — same mechanism as upstream's
# unmodified browser path. node-test-harness.js supplies a focused tape
# adapter on jsse; Node loads real tape as an independent framework oracle.
#
# Curve25519/Ed25519 point arithmetic is ~100-3500x slower on the tree-walker
# than V8 per operation (a single scalarMult.base ≈ 3.4s here vs ≈35ms on
# Node; sign.detached.verify ≈ 12s vs ≈76ms). At the full upstream vector
# counts (256 scalarmult / 256 box / 1024 sign) that's on the order of ~7h,
# not minutes. A correctness smoke run against all 13 files with truncated
# vectors passed 1233/1233 on jsse, byte-identical to Node, so this is pure
# interpretation overhead rather than an engine bug — but a multi-hour harness
# isn't practical to actually run. lib_prepare therefore evenly samples the
# three curve-heavy vector files (scalarmult.random/box.random/sign.spec) down
# to 20 each (stride-sampled across the full array, not just a prefix, so the
# subset still spans the original vector space); every other file (secretbox,
# hash, onetimeauth — no elliptic-curve cost) stays at its full upstream
# count. This is the first sampled corpus in this harness (every other config
# runs its library's suite unmodified) — exhaustive coverage is tracked in
# issue #361, to revisit once the engine has a faster numeric path.
LIB_REPO="https://github.com/dchest/tweetnacl-js.git"
LIB_REF="1.0.3"   # git tag; matches the published npm 1.0.3 exactly (same commit as v1.0.2)
LIB_ENTRY="test/jsse-entry.js"
LIB_ESBUILD_PLATFORM="node"
LIB_ESBUILD_EXTRA=(
    --alias:tape=./test/jsse-tape.js
)
LIB_SHIMS=("node-crypto-shim.js" "node-test-harness.js")
LIB_EXPECT_COUNT="5470"   # locked: sampled corpus, equal on jsse and Node
LIB_TIMEOUT="3600"        # 1h: the fixed 200-iteration scalarMult.base KAT loop alone is ~11min

lib_prepare() {
    # Retain only the dependencies the test files themselves import; the
    # browserify/eslint/uglify-js build toolchain is unrelated to the runtime
    # corpus.
    node -e "const p=require('./package.json'); p.devDependencies={}; p.scripts={}; require('fs').writeFileSync('package.json', JSON.stringify(p,null,2)+'\n')"
    npm install --no-save --no-audit --no-fund tape@5.10.2 tweetnacl-util@0.15.1
    node -e "
      const fs = require('fs');
      const files = fs.readdirSync('test').filter(f => /^[0-9].*\.js\$/.test(f));
      const needle = \"var nacl = (typeof window !== 'undefined') ? window.nacl : require('../' + (process.env.NACL_SRC || 'nacl.min.js'));\";
      for (const f of files) {
        const p = 'test/' + f;
        const src = fs.readFileSync(p, 'utf8');
        if (!src.includes(needle)) throw new Error('expected require line not found in ' + p);
        fs.writeFileSync(p, src.replace(needle, 'var nacl = window.nacl;'));
      }
    "
    # Evenly sample the curve-heavy vector files (see the header comment for
    # why); every other data file keeps its full upstream vector count.
    node -e "
      const fs = require('fs');
      function sample(arr, n) {
        if (arr.length <= n) return arr;
        var stride = arr.length / n, out = [];
        for (var i = 0; i < n; i++) out.push(arr[Math.floor(i * stride)]);
        return out;
      }
      var sampled = 0;
      ['test/data/scalarmult.random.js', 'test/data/box.random.js', 'test/data/sign.spec.js'].forEach(function (p) {
        var data = require('./' + p);
        var out = sample(data, 20);
        sampled += data.length - out.length;
        fs.writeFileSync(p, 'module.exports = ' + JSON.stringify(out, null, 2) + ';\n');
      });
      console.log('tweetnacl-js: dropped ' + sampled + ' vectors sampling to a tractable runtime (issue #361)');
    "
    cp "$SCRIPT_DIR/node-tape-module.js" test/jsse-tape.js
    cp "$SCRIPT_DIR/libs/tweetnacl-js-jsse-entry.js" "$LIB_ENTRY"
}

lib_verdict() {
    local out="$1" rc="$2" tests pass fail
    tests="$(grep -oE '# tests [0-9]+' "$out" | tail -1 | awk '{print $3}' || true)"
    pass="$(grep -oE '# pass[[:space:]]+[0-9]+' "$out" | tail -1 | awk '{print $3}' || true)"
    fail="$(grep -oE '# fail[[:space:]]+[0-9]+' "$out" | tail -1 | awk '{print $3}' || true)"
    fail="${fail:-0}"
    tests="${tests:-0}"
    pass="${pass:-0}"
    if [ "$rc" -eq 0 ] && [ "$tests" -gt 0 ] && [ "$pass" -eq "$tests" ] && [ "$fail" -eq 0 ]; then
        echo "PASS $tests"
        return 0
    fi
    echo "FAIL $tests"
    return 1
}
