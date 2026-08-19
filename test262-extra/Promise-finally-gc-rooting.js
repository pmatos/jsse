/*---
description: >
  Promise.prototype.finally keeps onFinally and the forwarded settlement value
  reachable across a collection that happens before the promise settles.
esid: sec-promise.prototype.finally
info: |
  ECMAScript 2024 §27.2.5.3 (Promise.prototype.finally).

  The thenFinally/catchFinally abstract closures capture onFinally and C, and
  the valueThunk/thrower they create capture the settlement value. jsse builds
  all four as native closures whose captures the collector cannot see, so each
  needs an explicit root. Found while fixing issue #309: without them a
  collection before the promise settles reclaims onFinally, the finally handler
  silently never runs, and the derived promise never settles.
flags: [async]
features: [Promise.prototype.finally, host-gc-required]
---*/

var release;
var derived;
var ranFinally = false;

// Scoped to an IIFE so no traced root keeps the source promise alive; see the
// same note in Promise-all-combinator-gc-rooting.js.
(function () {
  var source = new Promise(function (resolve) {
    release = resolve;
  });
  derived = source.finally(function () {
    ranFinally = true;
  });
})();

derived
  .then(function (value) {
    assert(ranFinally, "the onFinally callback ran");
    assert.sameValue(value.marker, "kept", "finally forwards the original value");
  })
  .then($DONE, $DONE);

// Collect while the source promise is still pending: at this point only
// thenFinally/catchFinally reference the onFinally callback.
$262.gc();

release({ marker: "kept" });
release = undefined;
