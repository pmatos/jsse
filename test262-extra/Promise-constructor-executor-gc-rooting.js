// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-promise-executor
description: >
  The promise under construction and its reject function stay reachable while
  the executor runs a garbage collection, even though the resolving functions
  capture only the promise's identity.
info: |
  Promise ( executor )

  7. Let promise be ? OrdinaryCreateFromConstructor(NewTarget, "%Promise.prototype%", « ... »).
  9. Let resolvingFunctions be CreateResolvingFunctions(promise).
  10. Let completion be Completion(Call(executor, undefined, « resolvingFunctions.[[Resolve]], resolvingFunctions.[[Reject]] »)).
  11. If completion is an abrupt completion, then
    a. Perform ? Call(resolvingFunctions.[[Reject]], undefined, « completion.[[Value]] »).
  12. Return promise.
features: [Promise, host-gc-required]
flags: [async]
---*/

function collect() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

var thenable = new Promise(function() {});
var p = new Promise(function(resolve) {
  resolve();
  collect();
  throw thenable;
});

assert.sameValue(typeof p.then, "function", "the constructed promise survives the collection");

var rejected = new Promise(function() {
  collect();
  throw new Test262Error("executor threw");
});

Promise.all([
  p,
  rejected.then(
    function() {
      throw new Test262Error("rejected promise must not fulfill");
    },
    function(reason) {
      assert.sameValue(reason.constructor, Test262Error, "the executor's error rejects the promise");
    }
  )
]).then(function() {
  $DONE();
}, $DONE);
