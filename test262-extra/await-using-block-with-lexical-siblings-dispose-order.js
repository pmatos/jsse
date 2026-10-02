// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-block-runtime-semantics-evaluation
description: >
  An `await using` block, or a `for (await using ...)` head, that shares its
  enclosing block, `try` clause or loop body with lexical declarations still
  disposes at its Await rather than inline, and the declarations keep their
  block scope.
info: |
  Block : { StatementList }

  [...]
  1. Let oldEnv be the running execution context's LexicalEnvironment.
  2. Let blockEnv be NewDeclarativeEnvironment(oldEnv).
  3. Perform BlockDeclarationInstantiation(StatementList, blockEnv).
  [...]
  6. Set blockValue to Completion(DisposeResources(blockEnv.[[DisposeCapability]], blockValue)).

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
  return { [Symbol.asyncDispose]() { L('disp' + (name === undefined ? '' : name)); } };
}

asyncTest(async function () {
  var log;

  log = await observe(function (L) {
    return (async function () {
      var i = 0;
      {
        let q = 'blk';
        for (await using a = D(L); i < 1; i++) {
          L('body-' + q);
        }
      }
      L('after-' + typeof q);
    })();
  });
  assert.compareArray(
    log,
    ['body-blk', 'disp', 'sync-end', 'w1', 'after-undefined', 'w2', 'settled', 'w3', 'w4'],
    'a let beside a head stays confined to its block'
  );

  log = await observe(function (L) {
    return (async function () {
      var i = 0;
      {
        const q = 'c';
        for (await using a = D(L); i < 1; i++) {
          L('body-' + q);
        }
        L('end-' + q);
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body-c', 'disp', 'sync-end', 'w1', 'end-c', 'after', 'w2', 'settled', 'w3', 'w4'],
    'a const beside a head is visible after the loop inside its block'
  );

  log = await observe(function (L) {
    return (async function () {
      var i = 0;
      {
        class K { static v() { return 'k'; } }
        for (await using a = D(L); i < 1; i++) {
          L('body-' + K.v());
        }
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body-k', 'disp', 'sync-end', 'w1', 'after', 'w2', 'settled', 'w3', 'w4'],
    'a class declaration beside a head is visible to the loop body'
  );

  log = await observe(function (L) {
    return (async function () {
      var i = 0;
      {
        function f() { return 'f'; }
        for (await using a = D(L); i < 1; i++) {
          L('body-' + f());
        }
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body-f', 'disp', 'sync-end', 'w1', 'after', 'w2', 'settled', 'w3', 'w4'],
    'a function declaration beside a head is visible to the loop body'
  );

  log = await observe(function (L) {
    return (async function () {
      {
        let x = 1;
        {
          await using a = D(L);
          x++;
        }
        L('x' + x);
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['disp', 'sync-end', 'w1', 'x2', 'after', 'w2', 'settled', 'w3', 'w4'],
    'a let beside an await using block is shared with it'
  );

  log = await observe(function (L) {
    return (async function () {
      let x = 'outer';
      {
        let x = 'inner';
        {
          await using a = D(L);
          L(x);
        }
      }
      L('after-' + x);
    })();
  });
  assert.compareArray(
    log,
    ['inner', 'disp', 'sync-end', 'w1', 'after-outer', 'w2', 'settled', 'w3', 'w4'],
    'an inner let shadows an outer one across an await using block'
  );

  log = await observe(function (L) {
    return (async function () {
      var f;
      {
        let x = 1;
        {
          await using a = D(L);
          f = () => x;
          x = 2;
        }
      }
      L('after-' + f());
    })();
  });
  assert.compareArray(
    log,
    ['disp', 'sync-end', 'w1', 'after-2', 'w2', 'settled', 'w3', 'w4'],
    'a closure keeps the shared binding alive past the block'
  );

  log = await observe(function (L) {
    return (async function () {
      try {
        let t = 1;
        {
          await using a = D(L);
          L('t' + t);
        }
      } finally {
        L('fin');
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['t1', 'disp', 'sync-end', 'w1', 'fin', 'after', 'w2', 'settled', 'w3', 'w4'],
    'a let in a try block beside an await using block disposes before the finally'
  );

  log = await observe(function (L) {
    return (async function () {
      try {
        throw 1;
      } catch (e) {
        let c = 'c';
        {
          await using a = D(L);
          L(c + e);
        }
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['c1', 'disp', 'sync-end', 'w1', 'after', 'w2', 'settled', 'w3', 'w4'],
    'a const in a catch body beside an await using block'
  );

  log = await observe(function (L) {
    return (async function () {
      for (var i = 0; i < 2; i++) {
        let j = i * 10;
        {
          await using a = D(L, i);
          L('j' + j);
        }
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['j0', 'disp0', 'sync-end', 'w1', 'j10', 'disp1', 'w2', 'after', 'w3', 'settled', 'w4'],
    'a let in a loop body beside an await using block disposes every iteration'
  );

  log = await observe(function (L) {
    return (async function () {
      try {
        {
          let x = 1;
          {
            await using a = D(L);
            L(typeof y);
            let y = 2;
          }
        }
      } catch (e) {
        L(e.constructor.name);
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['disp', 'sync-end', 'w1', 'ReferenceError', 'after', 'w2', 'settled', 'w3', 'w4'],
    'a later let in the enclosing block is in its temporal dead zone'
  );

  log = await observe(function (L) {
    return (async function () {
      var i = 0;
      {
        let a1 = 1;
        let b1 = 2;
        for (await using a = D(L); i < 1; i++) {
          L('s' + (a1 + b1));
        }
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['s3', 'disp', 'sync-end', 'w1', 'after', 'w2', 'settled', 'w3', 'w4'],
    'several lets beside a head all stay in the block'
  );
});
