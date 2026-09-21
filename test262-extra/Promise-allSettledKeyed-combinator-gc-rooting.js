/*---
description: >
  Promise.allSettledKeyed keeps its capability resolve function and its
  accumulated outcome records reachable across a collection that happens while
  the combinator is in flight.
esid: sec-promise.allsettledkeyed
info: |
  PerformPromiseAllSettledKeyed builds a fulfilled and a rejected element
  function per own key; each holds the shared [[Values]] list and the result
  capability. jsse builds those functions as native closures that capture the
  accumulator by value, where the collector cannot see it, so it needs an
  explicit root. A settled outcome must stay reachable through the combinator
  alone: once the input promise and its resolving function are dropped, nothing
  else holds it.
flags: [async]
features: [await-dictionary, host-gc-required]
---*/

var releaseFirst;
var rejectSecond;
var combined;

// The input promises are scoped to this IIFE on purpose. A top-level `var`
// binding for them would be traced as a GC root, so the test would pass even
// with the rooting under test removed.
(function () {
  var first = new Promise(function (resolve) {
    releaseFirst = resolve;
  });
  var second = new Promise(function (_, reject) {
    rejectSecond = reject;
  });
  combined = Promise.allSettledKeyed({ a: first, b: second });
})();

combined
  .then(function (results) {
    assert.sameValue(Object.keys(results).join(), "a,b", "both keys are reported");
    assert.sameValue(results.a.status, "fulfilled", "key a is fulfilled");
    assert.sameValue(results.a.value.marker, "first", "key a survived collection");
    assert.sameValue(results.b.status, "rejected", "key b is rejected");
    assert.sameValue(results.b.reason.marker, "second", "key b survived collection");
  })
  .then($DONE, $DONE);

// Collect while nothing has settled: only the element functions reference the
// capability's resolve function at this point.
$262.gc();

releaseFirst({ marker: "first" });
// Drop the last traced path to `first`, so its settled value is reachable only
// through the combinator's own accumulator.
releaseFirst = undefined;

// Queued after key a's reaction job, so that job has already stored its outcome
// record in the accumulator by the time this runs.
Promise.resolve().then(function () {
  $262.gc();
  rejectSecond({ marker: "second" });
  rejectSecond = undefined;
});
