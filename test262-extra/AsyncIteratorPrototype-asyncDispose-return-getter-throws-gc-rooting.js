// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
description: >
  The promise capability %AsyncIteratorPrototype%[Symbol.asyncDispose] creates
  up front stays reachable across a garbage collection triggered from within
  the `return` getter it reads, so a getter that collects and then throws
  still rejects the returned promise with its own error instead of the
  capability's promise being reclaimed and recycled (jsse#825).
esid: sec-%asynciteratorprototype%-@@asyncDispose
info: |
  %AsyncIteratorPrototype% [ @@asyncDispose ] ( )

  1. Let O be the this value.
  2. Let promiseCapability be ! NewPromiseCapability(%Promise%).
  3. Let return be GetMethod(O, "return").
  4. IfAbruptRejectPromise(return, promiseCapability).
  ...
flags: [async]
features: [explicit-resource-management, host-gc-required]
---*/

async function* generator() {}
const AsyncIteratorPrototype = Object.getPrototypeOf(Object.getPrototypeOf(generator.prototype));

function CatchError() {}

var returnGetCount = 0;

const obj = {
  get return() {
    returnGetCount++;
    $262.gc();
    throw new CatchError();
  }
};

const p = AsyncIteratorPrototype[Symbol.asyncDispose].call(obj);

assert.sameValue(
  Object.getPrototypeOf(p),
  Promise.prototype,
  "the result is still a promise after the getter's gc call"
);

p
  .then(
    function () {
      throw new Error("expected the promise to reject, but it fulfilled");
    },
    function (e) {
      assert.sameValue(e instanceof CatchError, true, "rejects with the return getter's own error");
      assert.sameValue(returnGetCount, 1, "the return getter ran exactly once");
    }
  )
  .then($DONE, $DONE);
