// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
description: >
  Objects already collected into a lowered array pattern's rest element
  survive collection during later iterator steps, across all three drivers.
esid: sec-runtime-semantics-iteratorbindinginitialization
info: |
  BindingRestElement and AssignmentRestElement retain each IteratorStepValue
  result in the rest array before calling the iterator again. A suspended
  default forces these patterns through their state-machine lowering.
flags: [async]
includes: [asyncHelpers.js]
features: [async-functions, async-iteration, generators, destructuring-binding, destructuring-assignment]
---*/

function iterable() {
  return {
    [Symbol.iterator]() {
      var index = 0;
      return {
        next() {
          $262.gc();
          if (index++ === 0) return { value: undefined, done: false };
          if (index <= 4) return { value: { marker: index }, done: false };
          return { done: true };
        },
        return() { throw new Test262Error('an exhausted rest iterator must not close'); }
      };
    }
  };
}

function verify(rest) {
  assert.sameValue(rest.length, 3);
  for (var i = 0; i < rest.length; i++) {
    assert.sameValue(rest[i].marker, i + 2, 'fresh rest object survives later next calls');
  }
}

asyncTest(async function () {
  for (var assignment of [false, true]) {
    var g = assignment ? (function* () {
      var first, rest;
      [first = yield 7, ...rest] = iterable();
      return rest;
    })() : (function* () {
      var [first = yield 7, ...rest] = iterable();
      return rest;
    })();
    assert.sameValue(g.next().value, 7);
    verify(g.next(7).value);

    var ag = assignment ? (async function* () {
      var first, rest;
      [first = await 7, ...rest] = iterable();
      return rest;
    })() : (async function* () {
      var [first = await 7, ...rest] = iterable();
      return rest;
    })();
    verify((await ag.next()).value);

    var result = assignment ? (async function () {
      var first, rest;
      [first = await 7, ...rest] = iterable();
      return rest;
    })() : (async function () {
      var [first = await 7, ...rest] = iterable();
      return rest;
    })();
    verify(await result);
  }
});
