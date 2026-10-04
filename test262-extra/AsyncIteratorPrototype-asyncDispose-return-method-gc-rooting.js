// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
description: >
  The promise capability %AsyncIteratorPrototype%[Symbol.asyncDispose] creates,
  along with the intermediate PromiseResolve wrapper and onFulfilled handler it
  threads through PerformPromiseThen, stay reachable across a garbage
  collection triggered from within the `return` method it calls. A `return()`
  that collects and then returns an ordinary (non-thenable) value must still
  fulfill the returned promise with undefined, rather than the capability's
  promise or intermediate values being reclaimed and recycled (jsse#825).
esid: sec-%asynciteratorprototype%-@@asyncDispose
info: |
  %AsyncIteratorPrototype% [ @@asyncDispose ] ( )

  1. Let O be the this value.
  2. Let promiseCapability be ! NewPromiseCapability(%Promise%).
  3. Let return be GetMethod(O, "return").
  4. IfAbruptRejectPromise(return, promiseCapability).
  5. If return is undefined, then
     a. Perform ! Call(promiseCapability.[[Resolve]], undefined, « undefined »).
  6. Else,
     a. Let result be Call(return, O, « »).
     b. IfAbruptRejectPromise(result, promiseCapability).
     c. Let resultWrapper be Completion(PromiseResolve(%Promise%, result)).
     d. IfAbruptRejectPromise(resultWrapper, promiseCapability).
     e. Let onFulfilled be a new Abstract Closure with no parameters that
        captures no values and performs the following steps when called:
        i. Return undefined.
     f. Let onFulfilledFunction be CreateBuiltinFunction(onFulfilled, 0, "", « »).
     g. Perform PerformPromiseThen(resultWrapper, onFulfilledFunction, undefined, promiseCapability).
  7. Return promiseCapability.[[Promise]].
flags: [async]
features: [explicit-resource-management, host-gc-required]
---*/

async function* generator() {}
const AsyncIteratorPrototype = Object.getPrototypeOf(Object.getPrototypeOf(generator.prototype));

var returnCallCount = 0;

const obj = {
  return: function () {
    returnCallCount++;
    $262.gc();
    return { marker: "not a thenable" };
  }
};

const p = AsyncIteratorPrototype[Symbol.asyncDispose].call(obj);

assert.sameValue(
  Object.getPrototypeOf(p),
  Promise.prototype,
  "the result is still a promise after the return method's gc call"
);

p
  .then(
    function (v) {
      assert.sameValue(v, undefined, "fulfills with undefined per step 7");
      assert.sameValue(returnCallCount, 1, "the return method ran exactly once");
    },
    function (e) {
      throw new Error("expected the promise to fulfill, but it rejected with " + e);
    }
  )
  .then($DONE, $DONE);
