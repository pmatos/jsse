// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-for-statement-runtime-semantics-forloopevaluation
description: >
  In an async generator, a `for (await using x = init; test; update)` head
  disposes its loop environment once, when the loop exits, before the
  statements after the loop run and at its Await rather than inline.
info: |
  ForStatement : for ( LexicalDeclaration Expression_opt ; Expression_opt ) Statement

  [...]
  8. Set bodyResult to DisposeResources(loopEnv.[[DisposeCapability]], bodyResult).

  A witness chain of promise reactions is started before the generator's
  consumer gets its own reaction, so the position of each entry pins the
  number of ticks every disposal consumed.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration]
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
  for (var i = 0; i < 14; i++) {
    drain = drain.then(function () {});
  }
  return drain.then(function () { return log; });
}

function D(L, name) {
  return { [Symbol.asyncDispose]() { L('disp' + (name === undefined ? '' : name)); } };
}

function run(ag, L) {
  return (async function () {
    for await (var v of ag()) L('got' + v);
    L('after');
  })();
}

asyncTest(async function () {
  var log = await observe(function (L) {
    return run(async function* () {
      var i = 0;
      for (await using a = D(L); i < 1; i++) {
        L('body');
      }
      L('ag-after');
      yield 1;
    }, L);
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'ag-after', 'w2', 'w3', 'got1', 'w4', 'after', 'settled'],
    'a head without a yield in the body disposes at its Await before the code after the loop'
  );

  log = await observe(function (L) {
    return run(async function* () {
      var i = 0;
      for (await using a = D(L); i < 2; i++) {
        L('body' + i);
        yield i;
      }
      L('ag-after');
    }, L);
  });
  assert.compareArray(
    log,
    ['body0', 'sync-end', 'w1', 'w2', 'got0', 'body1', 'w3', 'w4', 'got1', 'disp', 'ag-after', 'after', 'settled'],
    'a yield in the body keeps the single loop-exit disposal before the code after the loop'
  );

  log = await observe(function (L) {
    return run(async function* () {
      var i = 0;
      for (await using a = D(L); i < 3; i++) {
        L('body' + i);
        yield i;
        break;
      }
      L('ag-after');
    }, L);
  });
  assert.compareArray(
    log,
    ['body0', 'sync-end', 'w1', 'w2', 'got0', 'disp', 'w3', 'ag-after', 'w4', 'after', 'settled'],
    'break disposes the loop environment before the code after the loop'
  );

  log = await observe(function (L) {
    return run(async function* () {
      var i = 0;
      for (await using a = D(L); i < 3; i++) {
        L('body' + i);
        return 9;
      }
      L('ag-after');
    }, L);
  });
  assert.compareArray(
    log,
    ['body0', 'sync-end', 'w1', 'disp', 'w2', 'w3', 'after', 'w4', 'settled'],
    'return disposes the loop environment before the generator completes'
  );

  log = await observe(function (L) {
    return run(async function* () {
      var i = 0;
      try {
        for (await using a = D(L); i < 3; i++) {
          L('body' + i);
          yield i;
          throw new Error('b');
        }
      } catch (e) {
        L('caught');
      }
      L('ag-after');
    }, L);
  });
  assert.compareArray(
    log,
    ['body0', 'sync-end', 'w1', 'w2', 'got0', 'disp', 'w3', 'caught', 'ag-after', 'w4', 'after', 'settled'],
    'a throw disposes the loop environment before the catch runs'
  );

  log = await observe(function (L) {
    return run(async function* () {
      var n = 0;
      outer: for (var k = 0; k < 2; k++) {
        for (await using a = D(L, k); n < 5; n++) {
          L('body' + k);
          yield k;
          continue outer;
        }
      }
      L('ag-after');
    }, L);
  });
  assert.compareArray(
    log,
    ['body0', 'sync-end', 'w1', 'w2', 'got0', 'disp0', 'w3', 'body1', 'w4', 'got1', 'disp1', 'ag-after', 'after', 'settled'],
    'continue to an outer label disposes the inner loop environment each time'
  );

  log = await observe(function (L) {
    return run(async function* () {
      var i = 0;
      lbl: for (await using a = D(L); i < 3; i++) {
        L('body' + i);
        yield i;
        continue lbl;
      }
      L('ag-after');
    }, L);
  });
  assert.compareArray(
    log,
    ['body0', 'sync-end', 'w1', 'w2', 'got0', 'body1', 'w3', 'w4', 'got1', 'body2', 'got2', 'disp', 'ag-after', 'after', 'settled'],
    'a label on the loop itself still resolves continue to the loop'
  );

  log = await observe(function (L) {
    return (async function () {
      async function* ag() {
        var i = 0;
        for (await using a = D(L); i < 3; i++) {
          L('body' + i);
          yield i;
        }
        L('ag-after');
      }
      var it = ag();
      await it.next();
      await it.return(5);
      L('after-return');
    })();
  });
  assert.compareArray(
    log,
    ['body0', 'sync-end', 'w1', 'w2', 'w3', 'disp', 'w4', 'after-return', 'settled'],
    'generator.return() while suspended in the body disposes before the return settles'
  );

  log = await observe(function (L) {
    return run(async function* () {
      var i = 0;
      for (await using a = await D(L); i < 1; i++) {
        L('body');
        yield i;
      }
      L('ag-after');
    }, L);
  });
  assert.compareArray(
    log,
    ['sync-end', 'w1', 'body', 'w2', 'w3', 'got0', 'disp', 'w4', 'ag-after', 'after', 'settled'],
    'an awaiting initializer still registers the resource'
  );

  log = await observe(function (L) {
    return run(async function* () {
      var i = 0;
      if (true) {
        for (await using a = D(L); i < 1; i++) {
          L('body');
          yield i;
        }
      }
      L('ag-after');
    }, L);
  });
  assert.compareArray(
    log,
    ['body', 'sync-end', 'w1', 'w2', 'got0', 'disp', 'w3', 'ag-after', 'w4', 'after', 'settled'],
    'a head nested in an if disposes before the code after the if'
  );
});
