// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-yield
description: >
  `yield` in an async generator performs Await(operand) first, so a rejected
  operand throws at the `yield`: an enclosing try statement catches it, its
  finally block runs, and the generator stays usable. Without a handler the
  request is rejected and the generator completes.
info: |
  Yield ( value )

  1. Let generatorKind be GetGeneratorKind().
  2. If generatorKind is async, return ? AsyncGeneratorYield(? Await(value)).
  3. Otherwise, return ? GeneratorYield(CreateIterResultObject(value, false)).

flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

function thenableRejecting(reason) {
  return { then(_, reject) { reject(reason); } };
}

var cases = [];

var operands = {
  'already rejected promise': function (r) { return Promise.reject(r); },
  'pending promise rejected later': function (r) {
    return new Promise(function (_, reject) { Promise.resolve().then(function () { reject(r); }); });
  },
  'rejecting thenable': thenableRejecting
};

Object.keys(operands).forEach(function (name) {
  var make = operands[name];

  cases.push(async function () {
    var log = [];
    async function* g() {
      try {
        yield make('R');
        log.push('unreachable');
      } catch (e) {
        log.push('caught:' + e);
      } finally {
        log.push('finally');
      }
      var got = yield 'after';
      log.push('got:' + got);
      return 'done';
    }
    var it = g();
    var r1 = await it.next();
    assert.sameValue(r1.value, 'after', name + ': first request is served by the next yield');
    assert.sameValue(r1.done, false, name + ': first request done');
    var r2 = await it.next('S');
    assert.sameValue(r2.value, 'done', name + ': second request value');
    assert.sameValue(r2.done, true, name + ': second request done');
    assert.compareArray(log, ['caught:R', 'finally', 'got:S'], name + ': log');
  });

  cases.push(async function () {
    var log = [];
    async function* g() {
      try {
        var x = yield make('R');
        log.push('unreachable:' + x);
      } finally {
        log.push('finally');
      }
    }
    var it = g();
    var error;
    try {
      await it.next();
    } catch (e) {
      error = e;
    }
    assert.sameValue(error, 'R', name + ': unhandled rejection rejects the request');
    assert.compareArray(log, ['finally'], name + ': finally ran');
    var after = await it.next();
    assert.sameValue(after.done, true, name + ': generator is completed');
    assert.sameValue(after.value, undefined, name + ': completed value');
  });

  cases.push(async function () {
    var log = [];
    async function* g() {
      for (var i = 0; i < 3; i++) {
        try {
          yield make('R' + i);
        } catch (e) {
          log.push(e);
        }
      }
      yield 'end';
    }
    var it = g();
    var p1 = it.next();
    var p2 = it.next();
    var r1 = await p1;
    var r2 = await p2;
    assert.sameValue(r1.value, 'end', name + ': queued first request');
    assert.sameValue(r2.done, true, name + ': queued second request');
    assert.compareArray(log, ['R0', 'R1', 'R2'], name + ': every rejection caught in turn');
  });

  cases.push(async function () {
    var closed = 0;
    var iterable = {
      [Symbol.iterator]() {
        return { next() { return { value: 1, done: false }; }, return() { closed++; return {}; } };
      }
    };
    async function* g() {
      for (var v of iterable) {
        yield make('R');
      }
    }
    var error;
    try {
      await g().next();
    } catch (e) {
      error = e;
    }
    assert.sameValue(error, 'R', name + ': for-of body rejection propagates');
    assert.sameValue(closed, 1, name + ': for-of iterator closed');
  });
});

asyncTest(async function () {
  for (var i = 0; i < cases.length; i++) {
    await cases[i]();
  }
});
