// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-newpromisecapability
description: >
  The resolve and reject functions a custom Promise constructor passes to the
  GetCapabilitiesExecutor function stay reachable when the constructor runs a
  garbage collection between calling the executor and returning.
info: |
  NewPromiseCapability ( C )

  6. Let executor be CreateBuiltinFunction(executorClosure, 2, "", « »).
  7. Let promise be ? Construct(C, « executor »).
  8. If IsCallable(promiseCapability.[[Resolve]]) is false, throw a TypeError exception.
  9. If IsCallable(promiseCapability.[[Reject]]) is false, throw a TypeError exception.

  GetCapabilitiesExecutor Functions

  7. Set promiseCapability.[[Resolve]] to resolve.
  8. Set promiseCapability.[[Reject]] to reject.
features: [class, Promise, host-gc-required]
flags: [async]
---*/

var createCustom = false;
var resolved = [];
var promiseObject = {};

class P extends Promise {
  constructor(executor) {
    if (createCustom) {
      executor(
        function(value) {
          resolved.push(value);
        },
        function(reason) {
          $DONE(reason);
        }
      );
      $262.gc();
      var churn = [];
      for (var i = 0; i < 500; i++) {
        churn.push({ churn: i });
      }
      return promiseObject;
    }
    return super(executor);
  }
}

var p = P.resolve(1);

createCustom = true;
var q = p.then();
createCustom = false;

assert.sameValue(q, promiseObject, "then() returns the custom constructor's object");

p.then(function() {
  assert.sameValue(resolved.length, 1, "the custom resolve function was called once");
  assert.sameValue(resolved[0], 1, "with the fulfillment value");
}).then($DONE, $DONE);
