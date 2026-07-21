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

// nacl.min.js's own PRNG auto-init prefers the browser path: `typeof self !==
// 'undefined' ? (self.crypto || self.msCrypto) : null`, then
// `crypto.getRandomValues`. Setting `self` lets node-crypto-shim.js's Web
// Crypto shim (issue #229's __host_random_bytes syscall floor, already wired
// for the uuid harness) satisfy that check directly, so nacl configures its
// own PRNG with no jsse-specific code here. Node has had a native global
// `crypto` since Node 19, so this is a no-op there (nacl picks the native
// implementation, matching upstream's unmodified auto-init).
globalThis.self = globalThis;
globalThis.nacl = require("../nacl.min.js");

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
