/*---
description: >
  Array.fromAsync keeps its array-like source, mapfn, thisArg, result array and
  resolving functions reachable across collections that happen while an element
  Await is still pending.
esid: sec-array.fromasync
info: |
  Array.fromAsync is a continuation-passing native in jsse: after the first
  Await the only references to asyncItems, mapfn, thisArg, the constructed
  array and the promise capability live in Rust-side captures the collector
  cannot see. Each continuation reads them back by id, so a collection that
  reclaims any of them while an element is pending breaks the call.
flags: [async]
features: [Array.fromAsync, host-gc-required]
---*/

var releases = [];
var seenThis = [];
var result;

(function () {
  var thisArg = { marker: "this" };
  var items = {
    length: 2,
    0: new Promise(function (resolve) { releases[0] = resolve; }),
    1: new Promise(function (resolve) { releases[1] = resolve; }),
  };
  result = Array.fromAsync(
    items,
    function (value, index) {
      seenThis[index] = this.marker;
      return { mapped: value, index: index };
    },
    thisArg
  );
})();

result
  .then(function (arr) {
    assert.sameValue(arr.length, 2, "result length");
    assert.sameValue(arr[0].mapped, "first", "first element mapped");
    assert.sameValue(arr[0].index, 0, "first index");
    assert.sameValue(arr[1].mapped, "second", "second element mapped");
    assert.sameValue(arr[1].index, 1, "second index");
    assert.sameValue(seenThis[0], "this", "thisArg reached mapfn for element 0");
    assert.sameValue(seenThis[1], "this", "thisArg reached mapfn for element 1");
  })
  .then($DONE, $DONE);

$262.gc();
releases[0]("first");
void Promise.resolve().then(function () {
  $262.gc();
  releases[1]("second");
  $262.gc();
});
