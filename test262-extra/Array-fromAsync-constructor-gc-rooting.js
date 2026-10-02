/*---
description: >
  Array.fromAsync keeps the async iterator (or array-like), the result promise
  and its resolving functions reachable while the user-supplied `this`
  constructor runs and collects.
esid: sec-array.fromasync
info: |
  Construct(C) runs user code between obtaining the iterator (or ToObject of
  the array-like) and the first Await. Until the call state is built, the
  iterator, the promise and its resolving functions exist only as native locals
  the collector cannot see.
flags: [async]
features: [Array.fromAsync, host-gc-required]
---*/

function Collecting() {
  $262.gc();
}

var iterableResult;
var arrayLikeResult;

(function () {
  var iterable = {};
  iterable[Symbol.asyncIterator] = function () {
    var calls = 0;
    return {
      next: function () {
        return Promise.resolve({ done: calls++ > 0, value: "v" });
      },
    };
  };
  iterableResult = Array.fromAsync.call(Collecting, iterable);
  arrayLikeResult = Array.fromAsync.call(Collecting, { length: 1, 0: "a" });
})();

Promise.all([iterableResult, arrayLikeResult])
  .then(function (results) {
    assert.sameValue(results[0].length, 1, "async iterable length");
    assert.sameValue(results[0][0], "v", "async iterable element");
    assert.sameValue(results[1].length, 1, "array-like length");
    assert.sameValue(results[1][0], "a", "array-like element");
  })
  .then($DONE, $DONE);
