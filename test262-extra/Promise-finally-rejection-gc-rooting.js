/*---
description: >
  Promise.prototype.finally keeps catchFinally's captures and the rethrown
  reason reachable across a collection taken after the promise rejects but
  before catchFinally runs.
esid: sec-promise.prototype.finally
info: |
  ECMAScript 2024 §27.2.5.3 (Promise.prototype.finally) step 6.c.

  Rejecting the source clears its fulfill reactions, which drops thenFinally —
  and with it thenFinally's root on onFinally. From that moment catchFinally's
  own root is the only thing keeping onFinally alive, and the thrower created in
  step 6.c.iii is the only thing holding the rejection reason. Companion to
  Promise-finally-gc-rooting.js, which covers the fulfillment path.
flags: [async]
features: [Promise.prototype.finally, host-gc-required]
---*/

var rejectSource;
var derived;
var ranFinally = false;

// Scoped to an IIFE so no traced root keeps the source promise alive.
(function () {
  var source = new Promise(function (_resolve, reject) {
    rejectSource = reject;
  });
  derived = source.finally(function () {
    ranFinally = true;
  });
})();

derived
  .then(
    function () {
      throw new Error("finally must not swallow the rejection");
    },
    function (reason) {
      assert(ranFinally, "the onFinally callback ran");
      assert.sameValue(reason.marker, "boom", "finally rethrows the original reason");
    }
  )
  .then($DONE, $DONE);

// Queued *before* the rejection, so it runs after the reject has already
// dropped thenFinally but before catchFinally's own reaction job.
Promise.resolve().then(function () {
  $262.gc();
});

rejectSource({ marker: "boom" });
rejectSource = undefined;
