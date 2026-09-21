// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-yield
description: >
  A `yield` inside a destructuring-assignment default whose operand is a
  pending promise suspends the async generator until that promise settles;
  requests queued behind it settle afterwards, in order, and the sent value
  is still bound to the yield.
info: |
  Yield ( value )

  2. If generatorKind is async, return ? AsyncGeneratorYield(? Await(value)).

  AsyncGeneratorYield ( value )

  Await ( value )

  Await suspends the running execution context until the awaited promise
  settles.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration, destructuring-assignment]
---*/

var resolveOperand;
async function* g() {
  var a;
  ({a = yield new Promise(function (resolve) { resolveOperand = resolve; })} = {});
  return a;
}

asyncTest(async function () {
  var log = [];
  var it = g();
  var p1 = it.next().then(function (r) { log.push('first'); return r; });
  var p2 = it.next('sent').then(function (r) { log.push('second'); return r; });
  await Promise.resolve();
  await Promise.resolve();
  assert.compareArray(log, [], 'neither request settles while the operand is pending');
  resolveOperand('operand');
  var r1 = await p1;
  var r2 = await p2;
  assert.sameValue(r1.value, 'operand', 'first request yields the awaited operand');
  assert.sameValue(r1.done, false, 'first request is not done');
  assert.sameValue(r2.value, 'sent', 'the sent value is bound to the yield');
  assert.sameValue(r2.done, true, 'second request completes the generator');
  assert.compareArray(log, ['first', 'second'], 'requests settle in order');
});
