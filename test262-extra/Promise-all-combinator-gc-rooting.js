/*---
description: >
  Promise.all keeps its capability resolve function and its accumulated element
  values reachable across a collection that happens while the combinator is in
  flight.
esid: sec-promise.all
info: |
  ECMAScript 2024 §27.2.4.1 (Promise.all) / §27.2.4.1.3
  (Promise.all Resolve Element Functions).

  Each resolve element function holds [[Values]] (the shared List) and
  [[Capability]]. jsse builds those functions as native closures that capture
  the capability's resolve function and the accumulator by value, where the
  collector cannot see them, so both need explicit roots. Regression test for
  issue #309: a collection between setting the combinator up and its inputs
  settling reclaimed the capability resolve function, the element function then
  called a dead object, the combinator promise never settled, and every
  continuation awaiting it was lost.
flags: [async]
features: [host-gc-required]
---*/

var releaseFirst;
var releaseSecond;
var combined;

// The input promises are scoped to this IIFE on purpose. A top-level `var`
// binding for them would be traced as a GC root, so the test would pass even
// with the rooting under test removed.
(function () {
  var first = new Promise(function (resolve) {
    releaseFirst = resolve;
  });
  var second = new Promise(function (resolve) {
    releaseSecond = resolve;
  });
  combined = Promise.all([first, second]);
})();

combined
  .then(function (values) {
    assert.sameValue(values.length, 2, "both elements are reported");
    assert.sameValue(values[0].marker, "first", "element 0 survived collection");
    assert.sameValue(values[1].marker, "second", "element 1 survived collection");
  })
  .then($DONE, $DONE);

// Collect while nothing has settled: only the element functions reference the
// capability's resolve function at this point.
$262.gc();

releaseFirst({ marker: "first" });
// Drop the last traced path to `first`, so its settled value is reachable only
// through the combinator's own accumulator.
releaseFirst = undefined;

// Queued after element 0's reaction job, so that job has already stored its
// value in the accumulator by the time this runs.
Promise.resolve().then(function () {
  $262.gc();
  releaseSecond({ marker: "second" });
  releaseSecond = undefined;
});
