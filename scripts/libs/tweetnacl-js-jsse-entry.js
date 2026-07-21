// Bundle entry for tweetnacl-js's upstream tape suite: the same test/*.js set
// the `test-node` npm script runs (`tape test/*.js`). test/c/*.js needs a
// native addon built via its own Makefile and is out of scope, matching the
// project's existing no-native-addon precedent (js-sha256 skips its worker
// smoke test the same way).
//
// Every test file gates its `nacl` require behind
// `typeof window !== 'undefined' ? window.nacl : require('../' +
// (process.env.NACL_SRC || 'nacl.min.js'))` — a runtime string concatenation
// esbuild cannot resolve statically. Forcing the browser branch (set `window`,
// preload `nacl` once) keeps every test file's own require on the untaken
// ternary branch, so it never executes. This is the same trick
// js-sha256-jsse-entry.js uses for its browser/webpack mode.
globalThis.window = globalThis;
globalThis.nacl = require("../nacl.min.js");

// nacl.min.js's own PRNG auto-init picks Node's real crypto.randomBytes via a
// `typeof require` probe. esbuild compiles that probe against its own
// external-require helper (always defined), so the probe passes but the
// subsequent call throws "Dynamic require of crypto is not supported" without
// the alias below — and even with it, jsse has no real entropy source behind
// a bare `require`. Wire the #229 syscall floor directly instead; Node is
// unaffected (this only runs under jsse's --node host mode) and keeps using
// its own auto-init.
if (typeof __host_write !== "undefined") {
  nacl.setPRNG(function (x, n) {
    var v = __host_random_bytes(n);
    for (var i = 0; i < n; i++) x[i] = v[i];
  });
}

require("./00-api.js");
require("./01-verify.quick.js");
require("./02-randombytes.quick.js");
require("./03-onetimeauth.quick.js");
require("./04-secretbox.js");
require("./04-secretbox.quick.js");
require("./05-scalarmult.js");
require("./06-box.js");
require("./06-box.quick.js");
require("./07-hash.js");
require("./07-hash.quick.js");
require("./08-sign.js");
require("./08-sign.quick.js");
