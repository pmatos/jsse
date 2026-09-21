// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-yield
description: >
  A `yield` inside a destructuring-assignment default whose operand is a
  rejected promise rejects the request with the reason and completes the async
  generator.
info: |
  Yield ( value )

  2. If generatorKind is async, return ? AsyncGeneratorYield(? Await(value)).

  Await ( value )

  5. Let onRejected be a new Abstract Closure ... that resumes the suspended
     execution context with a throw completion.
flags: [async]
includes: [asyncHelpers.js]
features: [async-iteration, destructuring-assignment]
---*/

var reason = { name: 'operand reason' };
async function* g() {
  var a;
  ({a = yield Promise.reject(reason)} = {});
  yield 'unreachable';
}

asyncTest(async function () {
  var it = g();
  var caught;
  try {
    await it.next();
  } catch (e) {
    caught = e;
  }
  assert.sameValue(caught, reason, 'the request rejects with the operand reason');
  var r = await it.next();
  assert.sameValue(r.done, true, 'the generator is completed');
  assert.sameValue(r.value, undefined, 'the completed generator yields undefined');
});
