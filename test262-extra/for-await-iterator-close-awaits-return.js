// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asynciteratorclose
description: >
  A `for await` loop left early (break, return, throw, labelled continue)
  closes its iterator with AsyncIteratorClose, which Awaits the result of
  `return()` before the statements after the loop run, in async functions and
  in async generators alike.
info: |
  AsyncIteratorClose ( iteratorRecord, completion )

  [...]
  5. If innerResult is a normal completion, then
     a. Let return be innerResult.[[Value]].
     b. If return is undefined, return ? completion.
     c. Set innerResult to Completion(Call(return, iterator)).
     d. If innerResult is a normal completion, set innerResult to Completion(Await(innerResult.[[Value]])).
  6. If completion is a throw completion, return ? completion.
  7. If innerResult is a throw completion, return ? innerResult.
  8. If innerResult.[[Value]] is not an Object, throw a TypeError exception.
  9. Return ? completion.

  A witness chain of promise reactions is started before the function's
  promise gets its own reaction, so the position of each entry pins the
  number of ticks every close consumed.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
---*/

function asyncIter(L, retv) {
  return {
    [Symbol.asyncIterator]() {
      var n = 0;
      return {
        next() { return Promise.resolve({ value: n++, done: false }); },
        return(v) {
          L('ret');
          return retv ? retv() : Promise.resolve({ done: true });
        }
      };
    }
  };
}

function syncIter(L, hasRet) {
  return {
    [Symbol.iterator]() {
      var n = 0;
      var it = { next() { return { value: n++, done: false }; } };
      if (hasRet) it.return = function () { L('sret'); return {}; };
      return it;
    }
  };
}

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
      for await (var x of [1, 2]) { L('body');
      break;
      } L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'w2',  'body',  'w3',  'after',  'w4',  'settled'],
    'break out of a for await over an array Awaits the close of the async-from-sync iterator'
  );

  log = await observe(function (L) {
    return (async function () {
      for await (var x of [1]) { L('body');
      } L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'w2',  'body',  'w3',  'w4',  'after',  'settled'],
    'running to completion needs no close'
  );

  log = await observe(function (L) {
    return (async function () {
      for await (var x of syncIter(L, true)) { L('body');
      break;
      } L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'w2',  'body',  'sret',  'w3',  'w4',  'after',  'settled'],
    'a sync iterator return() is called and its result awaited by for await'
  );

  log = await observe(function (L) {
    return (async function () {
      for await (var x of syncIter(L, false)) { L('body');
      break;
      } L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'w2',  'body',  'w3',  'after',  'w4',  'settled'],
    'a sync iterator without return() still awaits the async-from-sync close'
  );

  log = await observe(function (L) {
    return (async function () {
      for await (var x of asyncIter(L)) { L('body');
      break;
      } L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'body',  'ret',  'w2',  'after',  'w3',  'settled',  'w4'],
    'break Awaits the result of an async iterator return()'
  );

  log = await observe(function (L) {
    return (async function () {
      for await (var x of asyncIter(L)) { L('body');
      return 1;
      }
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'body',  'ret',  'w2',  'w3',  'settled',  'w4'],
    'return Awaits the result of return() before the function settles'
  );

  log = await observe(function (L) {
    return (async function () {
      try { for await (var x of asyncIter(L)) { L('body');
      throw new Error('b');
      } } catch (e) { L('caught');
      } L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'body',  'ret',  'w2',  'caught',  'after',  'w3',  'settled',  'w4'],
    'a throw from the body Awaits return() before the catch runs'
  );

  log = await observe(function (L) {
    return (async function () {
      o: for (var i = 0;
      i < 1;
      i++) { for await (var x of asyncIter(L)) { L('body');
      continue o;
      } } L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'body',  'ret',  'w2',  'after',  'w3',  'settled',  'w4'],
    'continue to an outer label Awaits the inner loop close'
  );

  log = await observe(function (L) {
    return (async function () {
      for await (var x of asyncIter(L, () => new Promise(r => Promise.resolve().then(() => r({ done: true }))))) { L('body');
      break;
      } L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'body',  'ret',  'w2',  'w3',  'after',  'w4',  'settled'],
    'a return() promise that settles later holds the function until it does'
  );

  log = await observe(function (L) {
    return (async function () {
      async function* g() { try { yield 1;
      yield 2;
      } finally { L('gen-fin');
      } } for await (var x of g()) { L('body');
      break;
      } L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'w2',  'body',  'w3',  'gen-fin',  'w4',  'after',  'settled'],
    'break out of a for await over an async generator runs its finally before the code after the loop'
  );

  log = await observe(function (L) {
    return (async function () {
      if (true) { for await (var x of asyncIter(L)) { L('body');
      break;
      } } L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'body',  'ret',  'w2',  'after',  'w3',  'settled',  'w4'],
    'a for await nested in an if Awaits the close'
  );

  log = await observe(function (L) {
    return (async function () {
      async function* ag() { for await (var x of asyncIter(L)) { L('body');
      break;
      } L('ag-after');
      yield 1;
      } for await (var v of ag()) L('got' + v);
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'body',  'ret',  'w2',  'ag-after',  'w3',  'w4',  'got1',  'after',  'settled'],
    'inside an async generator, break Awaits return() before running the code after the loop'
  );

  log = await observe(function (L) {
    return (async function () {
      async function* ag() { for await (var x of asyncIter(L)) { L('body');
      return 7;
      } } for await (var v of ag()) L('got' + v);
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'body',  'w2',  'ret',  'w3',  'w4',  'after',  'settled'],
    'inside an async generator, return Awaits return() before the generator completes'
  );

  log = await observe(function (L) {
    return (async function () {
      async function* ag() { try { for await (var x of asyncIter(L)) { L('body');
      throw new Error('b');
      } } catch (e) { L('caught');
      } L('ag-after');
      yield 1;
      } for await (var v of ag()) L('got' + v);
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'body',  'ret',  'w2',  'caught',  'ag-after',  'w3',  'w4',  'got1',  'after',  'settled'],
    'inside an async generator, a throw Awaits return() before the catch runs'
  );

  log = await observe(function (L) {
    return (async function () {
      async function* g() { try { yield 1;
      yield 2;
      } finally { L('gen-fin');
      } } async function* ag() { for await (var x of g()) { L('body');
      break;
      } L('ag-after');
      yield 1;
      } for await (var v of ag()) L('got' + v);
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'w2',  'body',  'w3',  'gen-fin',  'w4',  'ag-after',  'got1',  'after',  'settled'],
    'inside an async generator, break over an async generator runs its finally first'
  );
});
