// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-with-statement-runtime-semantics-evaluation
description: >
  An `await using` declaration directly inside a block that is the body of a
  `with` statement registers its resource on that block's own environment, so
  the block's DisposeResources suspends the async function at its Await
  instead of draining the job queue inline.
info: |
  WithStatement : with ( Expression ) Statement

  [...]
  7. Set the running execution context's LexicalEnvironment to newEnv.
  8. Let C be Completion(Evaluation of Statement).

  Block : { StatementList }

  [...]
  4. Let blockValue be Completion(Evaluation of StatementList).
  5. Set blockValue to DisposeResources(blockEnv.[[DisposeCapability]], blockValue).

  DisposeResources ( disposeCapability, completion )

  [...]
  3. For each element resource of disposeCapability.[[DisposableResourceStack]], in reverse list order, do
     [...]
     f. Else,
        i. Assert: hint is async-dispose.
        ii. Set needsAwait to true.
  4. If needsAwait is true and hasAwaited is false, then
     a. Perform ! Await(undefined).
flags: [async, noStrict]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management]
---*/

function observe(shape) {
  var log = [];
  var L = function (entry) { log.push(entry); };
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
  var log = await observe(function (L) {
    return (async function () {
      with ({}) {
        await using a = { [Symbol.asyncDispose]() { L('disp'); } };
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['disp', 'sync-end', 'after', 'settled'],
    'the with body block suspends at its disposal Await'
  );

  log = await observe(function (L) {
    return (async function () {
      with ({ x: 'ox' }) {
        await using a = { [Symbol.asyncDispose]() { L('disp-' + x); } };
        L('body-' + x);
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body-ox', 'disp-ox', 'sync-end', 'after', 'settled'],
    'the block body and the disposer still resolve names through the with object'
  );

  log = await observe(function (L) {
    return (async function () {
      with ({}) {
        await using a = { [Symbol.asyncDispose]() { L('disp'); } };
        await 0;
        L('body');
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end', 'body', 'disp', 'after', 'settled'],
    'an await in the body runs before the block disposes'
  );

  log = await observe(function (L) {
    return (async function () {
      with ({}) {
        with ({ y: 'oy' }) {
          await using a = { [Symbol.asyncDispose]() { L('disp-' + y); } };
        }
        L('mid');
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['disp-oy', 'sync-end', 'mid', 'after', 'settled'],
    'nested with statements both enclose the scope'
  );

  log = await observe(function (L) {
    return (async function () {
      {
        await using a = { [Symbol.asyncDispose]() { L('disp-outer'); } };
        with ({ z: 'oz' }) {
          await using b = { [Symbol.asyncDispose]() { L('disp-' + z); } };
        }
        L('mid');
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['disp-oz', 'sync-end', 'mid', 'disp-outer', 'after', 'settled'],
    'a with inside an await-using block disposes inner then outer'
  );

  log = await observe(function (L) {
    return (async function () {
      try {
        with ({}) {
          await using a = { [Symbol.asyncDispose]() { L('disp'); } };
          throw new Error('boom');
        }
      } catch (e) {
        L('caught-' + e.message);
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['disp', 'sync-end', 'caught-boom', 'after', 'settled'],
    'an abrupt exit from the with body still disposes before the catch'
  );

  log = await observe(function (L) {
    return (async function () {
      var x = 'outer';
      with ({ x: 'ox' }) {
        let x = 'block';
        await using a = { [Symbol.asyncDispose]() { L('disp-' + x); } };
        L('body-' + x);
      }
      L('after-' + x);
    })();
  });
  assert.compareArray(
    log,
    ['body-block', 'disp-block', 'sync-end', 'after-outer', 'settled'],
    'a block-scoped binding shadows the with object'
  );

  log = await observe(function (L) {
    return (async function () {
      var o = { x: 'ox', [Symbol.unscopables]: { x: true } };
      var x = 'outer';
      with (o) {
        await using a = { [Symbol.asyncDispose]() { L('disp-' + x); } };
        L('body-' + x);
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body-outer', 'disp-outer', 'sync-end', 'after', 'settled'],
    'Symbol.unscopables is honoured inside the scope'
  );

  log = await observe(function (L) {
    return (async function () {
      for (var i = 0; i < 2; i++) {
        with ({ i: 'oi' }) {
          await using a = { [Symbol.asyncDispose]() { L('disp-' + i); } };
          L('body-' + i);
        }
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body-oi', 'disp-oi', 'sync-end', 'body-oi', 'disp-oi', 'after', 'settled'],
    'the scope is re-entered under the with on each loop iteration'
  );

  log = await observe(function (L) {
    return (async function () {
      try {
        with (null) {
          await using a = { [Symbol.asyncDispose]() { L('disp'); } };
          L('body');
        }
      } catch (e) {
        L('caught-' + (e instanceof TypeError));
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['caught-true', 'after', 'sync-end', 'settled'],
    'a null with operand throws a TypeError before the body runs'
  );
});
