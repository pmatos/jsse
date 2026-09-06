// tweetnacl-js curve25519/Ed25519 microbenchmark (jsse issue #361).
// Deterministic inputs only: no PRNG, so jsse and Node do identical work.
globalThis.self = globalThis;
var PHASE = "scalarmult_base", SM_ITERS = 5, SV_ITERS = 3, CTRL_ITERS = 200;

var nacl = self.nacl || (typeof module !== "undefined" ? module.exports : undefined);

function fill(n, seed) {
  var a = new Uint8Array(n);
  for (var i = 0; i < n; i++) a[i] = (i * 37 + seed * 101 + 7) & 0xff;
  return a;
}

var OUT = (typeof print === "function") ? print : function (s) { console.log(s); };

function bench(label, iters, fn) {
  var t0 = Date.now();
  var sink = 0;
  for (var i = 0; i < iters; i++) sink ^= fn(i);
  var dt = Date.now() - t0;
  OUT(label + "\t" + iters + "\t" + dt + "\t" + (dt / iters).toFixed(2) + "\tsink=" + sink);
}

var sk = fill(32, 1);
sk[0] &= 248; sk[31] &= 127; sk[31] |= 64;
var peer = nacl.scalarMult.base(fill(32, 2));

var signSeed = fill(32, 3);
var signKp = nacl.sign.keyPair.fromSeed(signSeed);
var msg = fill(128, 4);
var sig = nacl.sign.detached(msg, signKp.secretKey);

var key = fill(32, 5);
var nonce = fill(24, 6);
var boxed = nacl.secretbox(msg, nonce, key);

var PHASES = {
  scalarmult_base: function () { bench("scalarMult.base", SM_ITERS, function (i) { sk[0] = (sk[0] + 8) & 0xf8; return nacl.scalarMult.base(sk)[0]; }); },
  scalarmult: function () { bench("scalarMult", SM_ITERS, function (i) { sk[0] = (sk[0] + 8) & 0xf8; return nacl.scalarMult(sk, peer)[0]; }); },
  sign_verify: function () { bench("sign.detached.verify", SV_ITERS, function (i) { return nacl.sign.detached.verify(msg, sig, signKp.publicKey) ? 1 : 0; }); },
  sign: function () { bench("sign.detached", SV_ITERS, function (i) { msg[0] = i & 0xff; return nacl.sign.detached(msg, signKp.secretKey)[0]; }); },
  secretbox: function () { bench("secretbox.open", CTRL_ITERS, function (i) { return nacl.secretbox.open(boxed, nonce, key)[0]; }); },
  hash: function () { bench("hash", CTRL_ITERS, function (i) { msg[1] = i & 0xff; return nacl.hash(msg)[0]; }); }
};

PHASES[PHASE]();
