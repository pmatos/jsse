// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorunwrapyieldresumption
description: >
  When a `for await` loop exits early, AsyncIteratorClose calls the iterator's
  `return` method and awaits the result; the async generator's `return`
  completion at a suspended `yield` is itself awaited by
  AsyncGeneratorUnwrapYieldResumption before its `finally` block runs. The
  generator's `finally` therefore lands after the second witness tick, not
  before.
info: |
  AsyncGeneratorUnwrapYieldResumption ( resumptionValue )

  1. If resumptionValue is not a return completion, return ? resumptionValue.
  2. Let awaited be Completion(Await(resumptionValue.[[Value]])).
  3. If awaited is a throw completion, return ? awaited.
  4. Assert: awaited is a normal completion.
  5. Return Completion Record { [[Type]]: return, [[Value]]: awaited.[[Value]], [[Target]]: empty }.

  AsyncIteratorClose ( iteratorRecord, completion )

  [...]
  5. Let innerResult be Completion(GetMethod(iterator, "return")).
  6. If innerResult is a normal completion, then
     [...]
     c. Set innerResult to Completion(Call(return, iterator)).
     d. If innerResult is a normal completion, set innerResult to Completion(Await(innerResult.[[Value]])).

  A witness chain of promise reactions is started before the function's
  promise gets its own reaction, so the position of "iter-finally", "after"
  and "settled" pins the number of ticks each close consumed.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

function observe(shape) {
  var log = [];
  var L = function (entry) { log.push(entry); };
  Promise.resolve()
    .then(function () { L('w1'); })
    .then(function () { L('w2'); })
    .then(function () { L('w3'); })
    .then(function () { L('w4'); });
  var promise = shape(L);
  promise.then(function () { L('settled'); }, function () { L('rejected'); });
  L('sync-end');
  var drain = Promise.resolve();
  for (var i = 0; i < 12; i++) {
    drain = drain.then(function () {});
  }
  return drain.then(function () { return log; });
}

asyncTest(async function () {
  var log;

  log = await observe(function (L) {
    return (async function () {
      async function* gen() {
        try {
          yield 1;
        } finally {
          L('iter-finally');
        }
      }
      for await (const v of gen()) { L('body'); break; }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end', 'w1', 'w2', 'body', 'w3', 'iter-finally', 'w4', 'after', 'settled'],
    'break from `for await` closes the iterator through an Await, so the generator finally block runs after w3'
  );

  log = await observe(function (L) {
    return (async function () {
      async function* gen() {
        try {
          yield 1;
        } finally {
          L('iter-finally');
        }
      }
      for await (const v of gen()) { L('body'); return 'r'; }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end', 'w1', 'w2', 'body', 'w3', 'iter-finally', 'w4', 'settled'],
    'return from the loop body closes the iterator at the same tick as break'
  );

  log = await observe(function (L) {
    return (async function () {
      async function* gen() {
        try {
          yield 1;
        } finally {
          L('iter-finally');
        }
      }
      try {
        for await (const v of gen()) { L('body'); throw new Error('b'); }
      } catch (e) {
        L('caught-' + e.message);
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end', 'w1', 'w2', 'body', 'w3', 'iter-finally', 'w4', 'caught-b', 'after', 'settled'],
    'a throw from the loop body closes the iterator before the catch runs'
  );

  log = await observe(function (L) {
    return (async function () {
      async function* gen() {
        try {
          yield 1;
        } finally {
          L('iter-finally');
        }
      }
      outer: for (var k = 0; k < 2; k++) {
        for await (const v of gen()) { L('body' + k); continue outer; }
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end', 'w1', 'w2', 'body0', 'w3', 'iter-finally', 'w4', 'body1', 'iter-finally', 'after', 'settled'],
    'continue to an outer label closes the inner iterator once per exit'
  );

  log = await observe(function (L) {
    return (async function () {
      async function* gen() {
        yield 1;
      }
      for await (const v of gen()) { L('body'); break; }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end', 'w1', 'w2', 'body', 'w3', 'w4', 'after', 'settled'],
    'a generator without a finally still consumes the closing Await tick'
  );

  log = await observe(function (L) {
    return (async function () {
      async function* gen() {
        try {
          yield 1;
        } finally {
          L('f1');
          await 0;
          L('f2');
        }
      }
      for await (const v of gen()) { L('body'); break; }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end', 'w1', 'w2', 'body', 'w3', 'f1', 'w4', 'f2', 'after', 'settled'],
    'an await inside the generator finally block ticks after the closing Await'
  );

  log = await observe(function (L) {
    return (async function () {
      async function* gen() {
        try {
          yield 1;
        } finally {
          L('iter-finally');
        }
      }
      for await (const v of gen()) { L('body'); }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end', 'w1', 'w2', 'body', 'iter-finally', 'w3', 'after', 'w4', 'settled'],
    'normal exhaustion runs the finally on the next() resumption, one tick before early exit would'
  );

  log = await observe(function (L) {
    return (async function () {
      async function* gen() {
        try {
          yield 1;
        } finally {
          L('iter-finally');
        }
      }
      var g = gen();
      await g.next();
      await g.return();
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end', 'w1', 'w2', 'w3', 'iter-finally', 'w4', 'after', 'settled'],
    'an explicit g.return() on a suspended yield lands at the same tick as the loop early exit'
  );
});
