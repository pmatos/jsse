// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-block-runtime-semantics-evaluation
description: >
  A block, clause body or loop body that declares a synchronous `using`
  binding and also suspends (an `await` anywhere in the async function's
  statement) still disposes the binding when its scope exits, on every kind of
  exit.
info: |
  Block : { StatementList }

  [...]
  5. Let blockValue be Completion(Evaluation of StatementList).
  6. Set blockValue to Completion(DisposeResources(blockEnv.[[DisposeCapability]], blockValue)).
  7. Set the running execution context's LexicalEnvironment to oldEnv.
  8. Return ? blockValue.

  A witness chain of promise reactions is started before the function's
  promise gets its own reaction, so the position of each entry pins the
  number of ticks every disposal consumed.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management]
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

function D(L, name) {
  return { [Symbol.dispose]() { L('disp' + name); } };
}

asyncTest(async function () {
  var log;

  log = await observe(function (L) {
    return (async function () {
      { using a = D(L, 'A');
      await 0;
      L('body');
      } L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'body',  'dispA',  'after',  'w2',  'settled',  'w3',  'w4'],
    'a block whose body awaits disposes its using resource at block exit'
  );

  log = await observe(function (L) {
    return (async function () {
      { using a = { v: await 1, [Symbol.dispose]() { L('dispS');
      } };
      L('body' + a.v);
      } L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'body1',  'dispS',  'after',  'w2',  'settled',  'w3',  'w4'],
    'an await inside a using initializer does not skip the disposal'
  );

  log = await observe(function (L) {
    return (async function () {
      { using a = D(L, 'A');
      using b = D(L, 'B');
      await 0;
      L('body');
      } L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'body',  'dispB',  'dispA',  'after',  'w2',  'settled',  'w3',  'w4'],
    'using declarations dispose in reverse order'
  );

  log = await observe(function (L) {
    return (async function () {
      { using a = D(L, 'A');
      await using b = { [Symbol.asyncDispose]() { L('dispB');
      } };
      await 0;
      L('body');
      } L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'body',  'dispB',  'w2',  'dispA',  'after',  'w3',  'settled',  'w4'],
    'using and await using declarations share one disposal stack'
  );

  log = await observe(function (L) {
    return (async function () {
      try { using a = D(L, 'A');
      await 0;
      L('body');
      } finally { L('fin');
      } L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'body',  'dispA',  'fin',  'after',  'w2',  'settled',  'w3',  'w4'],
    'a try block disposes before its finally runs'
  );

  log = await observe(function (L) {
    return (async function () {
      try { throw 1;
      } catch (e) { using a = D(L, 'A');
      await 0;
      L('body');
      } L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'body',  'dispA',  'after',  'w2',  'settled',  'w3',  'w4'],
    'a catch body disposes at the clause exit'
  );

  log = await observe(function (L) {
    return (async function () {
      try { L('t');
      } finally { using a = D(L, 'A');
      await 0;
      L('body');
      } L('after');
    })();
  });
  assert.compareArray(
    log,
    ['t',  'sync-end',  'w1',  'body',  'dispA',  'after',  'w2',  'settled',  'w3',  'w4'],
    'a finally body disposes at the clause exit'
  );

  log = await observe(function (L) {
    return (async function () {
      for (var i = 0;
      i < 2;
      i++) { using a = D(L, 'A' + i);
      await 0;
      L('body' + i);
      break;
      } L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'body0',  'dispA0',  'after',  'w2',  'settled',  'w3',  'w4'],
    'break out of a loop body disposes the iteration scope'
  );

  log = await observe(function (L) {
    return (async function () {
      for (var i = 0;
      i < 2;
      i++) { using a = D(L, 'A' + i);
      await 0;
      L('body' + i);
      continue;
      } L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'body0',  'dispA0',  'w2',  'body1',  'dispA1',  'after',  'w3',  'settled',  'w4'],
    'continue disposes the iteration scope'
  );

  log = await observe(function (L) {
    return (async function () {
      { using a = D(L, 'A');
      await 0;
      return 'r';
      }
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'dispA',  'w2',  'settled',  'w3',  'w4'],
    'return from inside the block disposes before settling'
  );

  log = await observe(function (L) {
    return (async function () {
      try { { using a = D(L, 'A');
      await 0;
      throw new Error('t');
      } } catch (e) { L('caught');
      } L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'dispA',  'caught',  'after',  'w2',  'settled',  'w3',  'w4'],
    'a throw from inside the block disposes before the catch'
  );

  log = await observe(function (L) {
    return (async function () {
      try { { using a = { [Symbol.dispose]() { L('dispT');
      throw new Error('d');
      } };
      await 0;
      L('body');
      } } catch (e) { L('caught-' + e.message);
      } L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'body',  'dispT',  'caught-d',  'after',  'w2',  'settled',  'w3',  'w4'],
    'a throwing disposer surfaces as the completion'
  );

  log = await observe(function (L) {
    return (async function () {
      if (true) { using a = D(L, 'A');
      await 0;
      L('body');
      } L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'body',  'dispA',  'after',  'w2',  'settled',  'w3',  'w4'],
    'a block nested in an if disposes at its own exit'
  );

  log = await observe(function (L) {
    return (async function () {
      var a = 'outer';
      { using a = D(L, 'A');
      await 0;
      L('body');
      } L('after-' + a);
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'body',  'dispA',  'after-outer',  'w2',  'settled',  'w3',  'w4'],
    'the using binding does not clobber an outer binding'
  );

  log = await observe(function (L) {
    return (async function () {
      using a = D(L, 'A');
      await 0;
      L('body');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'body',  'dispA',  'w2',  'settled',  'w3',  'w4'],
    'a function-level using disposes at function exit'
  );

  log = await observe(function (L) {
    return (async function () {
      for (var i = 0;
      i < 2;
      i++) { using a = D(L, 'A' + i);
      await 0;
      L('body' + i);
      } L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end',  'w1',  'body0',  'dispA0',  'w2',  'body1',  'dispA1',  'after',  'w3',  'settled',  'w4'],
    'each loop iteration disposes its own scope'
  );
});
