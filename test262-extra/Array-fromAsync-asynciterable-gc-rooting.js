/*---
description: >
  Array.fromAsync keeps an async iterator, its mapfn, thisArg, result array and
  resolving functions reachable across collections that happen while a next()
  result or a mapped value is still pending.
esid: sec-array.fromasync
info: |
  After the first Await the async iterator, mapfn, thisArg, the constructed
  array and the promise capability are referenced only from Rust-side
  continuation captures the collector cannot see.
flags: [async]
features: [Array.fromAsync, host-gc-required]
---*/

var nextReleases = [];
var mapReleases = [];
var seenThis = [];
var result;

(function () {
  var thisArg = { marker: "this" };
  var calls = 0;
  var iterable = {};
  iterable[Symbol.asyncIterator] = function () {
    return {
      tag: "iterator",
      next: function () {
        var call = calls++;
        return new Promise(function (resolve) { nextReleases[call] = resolve; });
      },
    };
  };
  result = Array.fromAsync(
    iterable,
    function (value, index) {
      seenThis[index] = this.marker;
      return new Promise(function (resolve) { mapReleases[index] = resolve; });
    },
    thisArg
  );
})();

result
  .then(function (arr) {
    assert.sameValue(arr.length, 1, "result length");
    assert.sameValue(arr[0].mapped, "v", "mapped value");
    assert.sameValue(seenThis[0], "this", "thisArg reached mapfn");
  })
  .then($DONE, $DONE);

function afterTurns(n, fn) {
  var p = Promise.resolve();
  for (var i = 0; i < n; i++) p = p.then(function () {});
  p.then(fn);
}

$262.gc();
nextReleases[0]({ done: false, value: "v" });

afterTurns(6, function () {
  $262.gc();
  mapReleases[0]({ mapped: "v" });
  afterTurns(6, function () {
    $262.gc();
    nextReleases[1]({ done: true });
    $262.gc();
  });
});
