/*---
description: >
  Promise.any keeps its capability reject function and its accumulated errors
  reachable across a collection that happens while the combinator is in flight.
esid: sec-promise.any
info: |
  ECMAScript 2024 §27.2.4.3 (Promise.any) / §27.2.4.3.2 (Promise.any Reject
  Element Functions).

  Each reject element function holds [[Errors]] and [[Capability]]. jsse builds
  them as native closures that capture the capability's reject function and the
  errors accumulator by value, where the collector cannot see them, so both need
  explicit roots. Companion to Promise-all-combinator-gc-rooting.js; see issue
  #309 for the failure mode.
flags: [async]
features: [Promise.any, AggregateError, host-gc-required]
---*/

var rejectFirst;
var rejectSecond;
var combined;

// Scoped to an IIFE so no traced root keeps the inputs alive; a top-level `var`
// binding would make this pass even with the rooting under test removed.
(function () {
  var first = new Promise(function (_resolve, reject) {
    rejectFirst = reject;
  });
  var second = new Promise(function (_resolve, reject) {
    rejectSecond = reject;
  });
  combined = Promise.any([first, second]);
})();

combined
  .then(
    function () {
      throw new Error("Promise.any must reject when every input rejects");
    },
    function (error) {
      assert.sameValue(error.constructor, AggregateError, "rejects with AggregateError");
      assert.sameValue(error.errors.length, 2, "both errors are reported");
      assert.sameValue(error.errors[0].marker, "first", "error 0 survived collection");
      assert.sameValue(error.errors[1].marker, "second", "error 1 survived collection");
    }
  )
  .then($DONE, $DONE);

// Collect while nothing has settled: only the element functions reference the
// capability's reject function at this point.
$262.gc();

rejectFirst({ marker: "first" });
// Drop the last traced path to `first`, so its rejection reason is reachable
// only through the combinator's own errors accumulator.
rejectFirst = undefined;

// Queued after element 0's reaction job, so that job has already stored its
// reason in the accumulator by the time this runs.
Promise.resolve().then(function () {
  $262.gc();
  rejectSecond({ marker: "second" });
  rejectSecond = undefined;
});
