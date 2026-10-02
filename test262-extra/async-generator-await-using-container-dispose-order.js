// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-block-runtime-semantics-evaluation
description: >
  In an async generator, an `await using` block or `for (await using ...)`
  head inside a loop body, `try` clause or `switch` case disposes at its Await
  even when nothing in the container yields, so the statements after the block
  run only once its disposal has settled.
info: |
  Block : { StatementList }

  [...]
  6. Set blockValue to Completion(DisposeResources(blockEnv.[[DisposeCapability]], blockValue)).

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

function SD(L, name) {
  return { [Symbol.dispose]() { L('sdisp' + (name === undefined ? '' : name)); } };
}

function run(ag, L) {
  return (async function () {
    for await (var v of ag()) L('got' + v);
    L('after');
  })();
}

asyncTest(async function () {
  var log;

  log = await observe(function (L) {
    return run(async function* () {
        for (var i = 0; i < 2; i++) {
          await using a = D(L, i);
          L('body' + i);
        }
        L('ag-after');
        yield 1;
    }, L);
  });
  assert.compareArray(
    log,
    ['body0', 'disp0', 'sync-end', 'w1', 'body1', 'disp1', 'w2', 'ag-after', 'w3', 'w4', 'got1', 'after', 'settled'],
    'a block in a loop body disposes each iteration at its Await'
  );

  log = await observe(function (L) {
    return run(async function* () {
        for (var i = 0; i < 3; i++) {
          await using a = D(L, i);
          L('body' + i);
          break;
        }
        L('ag-after');
        yield 1;
    }, L);
  });
  assert.compareArray(
    log,
    ['body0', 'disp0', 'sync-end', 'w1', 'ag-after', 'w2', 'w3', 'got1', 'w4', 'after', 'settled'],
    'break out of a loop body disposes its block'
  );

  log = await observe(function (L) {
    return run(async function* () {
        for (var i = 0; i < 2; i++) {
          await using a = D(L, i);
          L('body' + i);
          continue;
        }
        L('ag-after');
        yield 1;
    }, L);
  });
  assert.compareArray(
    log,
    ['body0', 'disp0', 'sync-end', 'w1', 'body1', 'disp1', 'w2', 'ag-after', 'w3', 'w4', 'got1', 'after', 'settled'],
    'continue disposes the iteration block'
  );

  log = await observe(function (L) {
    return run(async function* () {
        var i = 0;
        while (i < 2) {
          {
            await using a = D(L, i);
            L('body' + i);
          }
          i++;
        }
        L('ag-after');
        yield 1;
    }, L);
  });
  assert.compareArray(
    log,
    ['body0', 'disp0', 'sync-end', 'w1', 'body1', 'disp1', 'w2', 'ag-after', 'w3', 'w4', 'got1', 'after', 'settled'],
    'a block in a while body disposes each iteration'
  );

  log = await observe(function (L) {
    return run(async function* () {
        try {
          {
            await using a = D(L);
            L('body');
          }
        } finally {
          L('fin');
        }
        L('ag-after');
        yield 1;
    }, L);
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'fin', 'ag-after', 'w2', 'w3', 'got1', 'w4', 'after', 'settled'],
    'a block in a try body disposes before the finally'
  );

  log = await observe(function (L) {
    return run(async function* () {
        try {
          await using a = D(L);
          L('body');
        } finally {
          L('fin');
        }
        L('ag-after');
        yield 1;
    }, L);
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'fin', 'ag-after', 'w2', 'w3', 'got1', 'w4', 'after', 'settled'],
    'a try clause body that declares await using disposes at the clause exit'
  );

  log = await observe(function (L) {
    return run(async function* () {
        try {
          throw 1;
        } catch (e) {
          await using a = D(L);
          L('body');
        }
        L('ag-after');
        yield 1;
    }, L);
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'ag-after', 'w2', 'w3', 'got1', 'w4', 'after', 'settled'],
    'a catch body that declares await using disposes at the clause exit'
  );

  log = await observe(function (L) {
    return run(async function* () {
        switch (1) {
          case 1: {
            await using a = D(L);
            L('body');
          }
          L('in-case');
        }
        L('ag-after');
        yield 1;
    }, L);
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'in-case', 'ag-after', 'w2', 'w3', 'got1', 'w4', 'after', 'settled'],
    'a block in a switch case disposes at its Await'
  );

  log = await observe(function (L) {
    return run(async function* () {
        for (var k = 0; k < 2; k++) {
          var j = 0;
          for (await using a = D(L, k); j < 1; j++) {
            L('body' + k);
          }
        }
        L('ag-after');
        yield 1;
    }, L);
  });
  assert.compareArray(
    log,
    ['body0', 'disp0', 'sync-end', 'w1', 'body1', 'disp1', 'w2', 'ag-after', 'w3', 'w4', 'got1', 'after', 'settled'],
    'a for head in a loop body disposes once per outer iteration'
  );

  log = await observe(function (L) {
    return run(async function* () {
        var i = 0;
        try {
          for (await using a = D(L); i < 1; i++) {
            L('body');
          }
        } finally {
          L('fin');
        }
        L('ag-after');
        yield 1;
    }, L);
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'fin', 'ag-after', 'w2', 'w3', 'got1', 'w4', 'after', 'settled'],
    'a for head in a try body disposes before the finally'
  );

  log = await observe(function (L) {
    return run(async function* () {
        var i = 0;
        switch (1) {
          case 1:
            for (await using a = D(L); i < 1; i++) {
              L('body');
            }
            L('in-case');
        }
        L('ag-after');
        yield 1;
    }, L);
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'in-case', 'ag-after', 'w2', 'w3', 'got1', 'w4', 'after', 'settled'],
    'a for head in a switch case disposes at its Await'
  );

  log = await observe(function (L) {
    return run(async function* () {
        for (var i = 0; i < 2; i++) {
          await using a = D(L, i);
          L('body' + i);
          return 5;
        }
        L('ag-after');
    }, L);
  });
  assert.compareArray(
    log,
    ['body0', 'sync-end', 'w1', 'disp0', 'w2', 'w3', 'after', 'w4', 'settled'],
    'return from a loop body disposes its block before the generator completes'
  );

  log = await observe(function (L) {
    return run(async function* () {
        try {
          for (var i = 0; i < 2; i++) {
            await using a = D(L, i);
            L('body' + i);
            throw new Error('t');
          }
        } catch (e) {
          L('caught');
        }
        L('ag-after');
        yield 1;
    }, L);
  });
  assert.compareArray(
    log,
    ['body0', 'disp0', 'sync-end', 'w1', 'caught', 'ag-after', 'w2', 'w3', 'got1', 'w4', 'after', 'settled'],
    'a throw from a loop body disposes its block before the catch'
  );

  log = await observe(function (L) {
    return run(async function* () {
        outer: for (var i = 0; i < 2; i++) {
          for (var j = 0; j < 2; j++) {
            await using a = D(L, i + '' + j);
            L('body' + i + j);
            break outer;
          }
        }
        L('ag-after');
        yield 1;
    }, L);
  });
  assert.compareArray(
    log,
    ['body00', 'disp00', 'sync-end', 'w1', 'ag-after', 'w2', 'w3', 'got1', 'w4', 'after', 'settled'],
    'break to an outer label disposes the inner block'
  );

  log = await observe(function (L) {
    return run(async function* () {
        {
          using a = SD(L);
          L('body');
          yield 1;
        }
        L('ag-after');
    }, L);
  });
  assert.compareArray(
    log,
    ['body', 'sync-end', 'w1', 'w2', 'got1', 'sdisp', 'ag-after', 'w3', 'after', 'w4', 'settled'],
    'a sync using block that yields disposes at its exit'
  );

  log = await observe(function (L) {
    return run(async function* () {
        for (var i = 0; i < 2; i++) {
          using a = SD(L, i);
          L('body' + i);
        }
        L('ag-after');
        yield 1;
    }, L);
  });
  assert.compareArray(
    log,
    ['body0', 'sdisp0', 'body1', 'sdisp1', 'ag-after', 'sync-end', 'w1', 'w2', 'got1', 'w3', 'after', 'w4', 'settled'],
    'a sync using in a loop body disposes each iteration'
  );

  log = await observe(function (L) {
    return run(async function* () {
        for (var i = 0; i < 2; i++) {
          await using a = D(L, i);
          L('body' + i);
          yield i;
        }
        L('ag-after');
    }, L);
  });
  assert.compareArray(
    log,
    ['body0', 'sync-end', 'w1', 'w2', 'got0', 'disp0', 'w3', 'body1', 'w4', 'got1', 'disp1', 'ag-after', 'after', 'settled'],
    'a yield in the loop body keeps each iteration disposal in order'
  );

  log = await observe(function (L) {
    return run(async function* () {
        for (var i = 0; i < 1; i++) {
          await using a = D(L, 'o');
          {
            await using b = D(L, 'i');
            L('body');
          }
          L('mid');
        }
        L('ag-after');
        yield 1;
    }, L);
  });
  assert.compareArray(
    log,
    ['body', 'dispi', 'sync-end', 'w1', 'mid', 'dispo', 'w2', 'ag-after', 'w3', 'w4', 'got1', 'after', 'settled'],
    'nested blocks dispose innermost first'
  );
});
