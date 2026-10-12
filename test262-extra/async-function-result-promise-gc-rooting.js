/*---
description: >
  An async function's result promise stays reachable while it is resolved with
  a thenable whose `then` accessor collects, even when the script keeps no
  reference to it.
esid: sec-async-function-definitions-InstantiateAsyncFunctionObject
info: |
  The result promise's resolving functions are engine-owned and never
  materialised as function objects. Resolving it with an object reads
  `then`, which runs user code that may collect; the promise must survive so
  the reaction registered on it still runs.
flags: [async]
features: [host-gc-required]
---*/

var thenReads = 0;

async function f(x) {
  await null;
  $262.gc();
  return {
    get then() {
      thenReads++;
      $262.gc();
      return function (resolve) { $262.gc(); resolve(x * 2); };
    }
  };
}

f(21).then(function (v) {
  assert.sameValue(v, 42);
  assert.sameValue(thenReads, 1);
}).then($DONE, $DONE);
