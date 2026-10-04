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
# Curve25519/Ed25519 point arithmetic runs ~140-390x slower on the tree-walker
# than on V8 per operation. At the 2026-09-05 baseline that put the full
# upstream counts (256 scalarmult / 256 box / 1024 sign) at a projected ~6h,
# so lib_prepare sampled all three curve-heavy vector files down to 20 each.
# #603 (bytecode `new`/compound-member-assignment support) has since landed,
# and general tree-walker throughput is also ~4x faster than that baseline
# (docs/perf/2026-10-04/tweetnacl-recheck.md) — re-measured end to end rather
# than re-projected: today's (then-)20/20/20 corpus runs in 9m25.7s real.
# scalarmult.random.js and box.random.js now run their full upstream
# 256-vector counts outright (sample()'s `arr.length <= n` guard makes the
# call for them a no-op). sign.spec.js is raised from 20 to 256 (of 1024
# upstream) — its per-vector cost (a sign + an open/verify, scaling linearly
# with vector count) is what keeps the total harness run inside LIB_TIMEOUT;
# 1024 projects to well over an hour for that one file alone. It is still
# stride-sampled (not a prefix) so the 256-vector subset spans the original
# vector space; every non-curve file (secretbox, hash, onetimeauth) already
# ran its full upstream count and is unaffected. Validated end to end
# (--clean, cold cache) at 50m15s real — this build host runs several
# concurrent agent sessions, so that figure includes some incidental
# contention (see the perf doc); LIB_TIMEOUT below is sized with real
# margin above it rather than against a best-case number.
#
# Exhaustive coverage (all three at full upstream counts) remains issue #361 —
# sign.spec.js is the long pole, not scalarmult/box, so closing it needs
# further engine throughput rather than another sampling-cap bump. The
# bytecode VM still doesn't help here: `car25519`'s `Math.floor(...)` call
# bails the compiler (`compile_call` only accepts an `Identifier` callee),
# which keeps 98.63% of the remaining tree-walked work off the VM — tracked in
# #839. Numbers, counter dumps and method:
# docs/perf/2026-10-04/tweetnacl-recheck.md.
LIB_REPO="https://github.com/dchest/tweetnacl-js.git"
LIB_REF="1.0.3"   # git tag; matches the published npm 1.0.3 exactly (same commit as v1.0.2)
LIB_ENTRY="test/jsse-entry.js"
LIB_ESBUILD_PLATFORM="node"
LIB_ESBUILD_EXTRA=(
    --alias:tape=./test/jsse-tape.js
)
LIB_SHIMS=("node-crypto-shim.js" "node-test-harness.js")
LIB_EXPECT_COUNT="7362"   # locked: raised corpus (#361), equal on jsse and Node
LIB_TIMEOUT="6000"        # 100min: measured 50m15s real on a loaded shared host (~2x margin)

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
    # Evenly sample sign.spec.js — the one file whose full upstream count
    # (1024) still doesn't fit LIB_TIMEOUT (see the header comment for why);
    # scalarmult.random.js and box.random.js now run their full upstream 256
    # each, unsampled.
    node -e "
      const fs = require('fs');
      function sample(arr, n) {
        if (arr.length <= n) return arr;
        var stride = arr.length / n, out = [];
        for (var i = 0; i < n; i++) out.push(arr[Math.floor(i * stride)]);
        return out;
      }
      var p = 'test/data/sign.spec.js';
      var data = require('./' + p);
      var out = sample(data, 256);
      fs.writeFileSync(p, 'module.exports = ' + JSON.stringify(out, null, 2) + ';\n');
      console.log('tweetnacl-js: dropped ' + (data.length - out.length) + ' sign.spec.js vectors sampling to a tractable runtime (issue #361)');
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
