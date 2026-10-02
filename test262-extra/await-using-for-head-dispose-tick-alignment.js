// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-for-statement-runtime-semantics-forloopevaluation
description: >
  A `for (await using x = init; test; update)` head disposes its loop
  environment once, when the loop exits, and that DisposeResources suspends
  the async function at its Await instead of draining the job queue inline,
  so the rest of the synchronous caller runs first.
info: |
  ForStatement : for ( LexicalDeclaration Expression_opt ; Expression_opt ) Statement

  [...]
  7. Let bodyResult be Completion(ForBodyEvaluation(test, increment, Statement, perIterationLets, labelSet)).
  8. Set bodyResult to DisposeResources(loopEnv.[[DisposeCapability]], bodyResult).
  9. Set the running execution context's LexicalEnvironment to oldEnv.
  10. Return ? bodyResult.

  DisposeResources ( disposeCapability, completion )

  [...]
  3. For each element resource of disposeCapability.[[DisposableResourceStack]], in reverse list order, do
     [...]
     f. Else,
        i. Assert: hint is async-dispose.
        ii. Set needsAwait to true.
  4. If needsAwait is true and hasAwaited is false, then
     a. Perform ! Await(undefined).

  A witness chain of promise reactions is started before the function's
  promise gets its own reaction, so the position of "after" and "settled"
  pins the number of ticks each disposal consumed.
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

asyncTest(async function () {
  var log = await observe(function (L) {
    return (async function () {
      var i = 0;
      for (await using a = null; i < 1; i++) {
        L('body');
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'sync-end', 'w1', 'after', 'w2', 'settled', 'w3', 'w4'],
    'a resource with no dispose method needs no Await'
  );

  log = await observe(function (L) {
    return (async function () {
      var i = 0;
      for (await using a = { [Symbol.asyncDispose]() { L('disp'); } }; i < 1; i++) {
        L('body');
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'after', 'w2', 'settled', 'w3', 'w4'],
    'an async disposer suspends the function at its Await'
  );

  log = await observe(function (L) {
    return (async function () {
      var i = 0;
      for (await using a = { [Symbol.asyncDispose]() { L('disp'); } }; i < 3; i++) {
        L('body' + i);
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body0', 'body1', 'body2', 'disp', 'sync-end', 'w1', 'after', 'w2', 'settled', 'w3', 'w4'],
    'the resource is disposed once, at loop exit, not once per iteration'
  );

  log = await observe(function (L) {
    return (async function () {
      var i = 0;
      for (await using a = null; i < 1; i++) {
        L('body');
        await 0;
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'sync-end', 'w1', 'w2', 'after', 'w3', 'settled', 'w4'],
    'an await in the body ticks independently of the head disposal'
  );

  log = await observe(function (L) {
    return (async function () {
      var i = 0;
      try {
        for (
          await using a = { [Symbol.asyncDispose]() { L('disp'); throw new Error('e1'); } };
          i < 1;
          i++
        ) {
          L('body');
        }
      } catch (e) {
        L('caught-' + e.message);
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'caught-e1', 'after', 'sync-end', 'w1', 'settled', 'w2', 'w3', 'w4'],
    'a synchronously-throwing disposer needs no Await'
  );

  log = await observe(function (L) {
    return (async function () {
      var i = 0;
      for (
        await using a = {
          [Symbol.asyncDispose]() {
            L('disp');
            return Promise.reject(new Error('e2'));
          },
        };
        i < 1;
        i++
      ) {
        L('body');
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'w2', 'rejected', 'w3', 'w4'],
    'a rejecting disposer promise suspends the function, which then rejects'
  );

  log = await observe(function (L) {
    return (async function () {
      var i = 0;
      for (
        await using a = { [Symbol.asyncDispose]() { L('dispA'); } },
          b = { [Symbol.asyncDispose]() { L('dispB'); } };
        i < 1;
        i++
      ) {
        L('body');
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'dispB', 'sync-end', 'w1', 'dispA', 'w2', 'after', 'w3', 'settled', 'w4'],
    'declarators dispose in reverse order, each suspending at its own Await'
  );

  log = await observe(function (L) {
    return (async function () {
      try {
        for (
          await using a = { [Symbol.asyncDispose]() { L('dispA'); } },
            b = (function () { throw new Error('init'); })();
          false;
        ) {
          L('body');
        }
      } catch (e) {
        L('caught-' + e.message);
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['dispA', 'sync-end', 'w1', 'caught-init', 'after', 'w2', 'settled', 'w3', 'w4'],
    'a throwing initializer disposes the earlier declarators before propagating'
  );

  log = await observe(function (L) {
    return (async function () {
      var i = 0;
      for (await using a = { [Symbol.asyncDispose]() { L('disp'); } }; i < 3; i++) {
        L('body' + i);
        break;
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body0', 'disp', 'sync-end', 'w1', 'after', 'w2', 'settled', 'w3', 'w4'],
    'break disposes the loop environment at its Await'
  );

  log = await observe(function (L) {
    return (async function () {
      var i = 0;
      for (await using a = { [Symbol.asyncDispose]() { L('disp'); } }; i < 3; i++) {
        L('body' + i);
        return 'r';
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body0', 'disp', 'sync-end', 'w1', 'w2', 'settled', 'w3', 'w4'],
    'return disposes the loop environment at its Await before settling'
  );

  log = await observe(function (L) {
    return (async function () {
      var i = 0;
      try {
        for (await using a = { [Symbol.asyncDispose]() { L('disp'); } }; i < 3; i++) {
          L('body' + i);
          throw new Error('b');
        }
      } catch (e) {
        L('caught-' + e.message);
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body0', 'disp', 'sync-end', 'w1', 'caught-b', 'after', 'w2', 'settled', 'w3', 'w4'],
    'a throw from the body disposes the loop environment at its Await first'
  );

  log = await observe(function (L) {
    return (async function () {
      var n = 0;
      outer: for (var k = 0; k < 2; k++) {
        for (await using a = { [Symbol.asyncDispose]() { L('disp' + k); } }; n < 5; n++) {
          L('body' + k);
          continue outer;
        }
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body0', 'disp0', 'sync-end', 'w1', 'body1', 'disp1', 'w2', 'after', 'w3', 'settled', 'w4'],
    'continue to an outer label disposes the inner loop environment each time'
  );

  log = await observe(function (L) {
    return (async function () {
      var i = 0;
      outer: for (var k = 0; k < 2; k++) {
        for (await using a = { [Symbol.asyncDispose]() { L('disp' + k); } }; i < 5; i++) {
          L('body' + k);
          break outer;
        }
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body0', 'disp0', 'sync-end', 'w1', 'after', 'w2', 'settled', 'w3', 'w4'],
    'break to an outer label disposes the inner loop environment'
  );

  log = await observe(function (L) {
    return (async function () {
      var i = 0;
      lbl: for (await using a = { [Symbol.asyncDispose]() { L('disp'); } }; i < 3; i++) {
        L('body' + i);
        continue lbl;
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body0', 'body1', 'body2', 'disp', 'sync-end', 'w1', 'after', 'w2', 'settled', 'w3', 'w4'],
    'a label on the loop itself still resolves continue to the loop'
  );

  log = await observe(function (L) {
    return (async function () {
      var i = 0;
      if (true) {
        for (await using a = { [Symbol.asyncDispose]() { L('disp'); } }; i < 1; i++) {
          L('body');
        }
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body', 'disp', 'sync-end', 'w1', 'after', 'w2', 'settled', 'w3', 'w4'],
    'a head nested in an if still suspends at its disposal'
  );

  log = await observe(function (L) {
    return (async function () {
      for (var k = 0; k < 2; k++) {
        var j = 0;
        for (await using a = { [Symbol.asyncDispose]() { L('disp' + k); } }; j < 1; j++) {
          L('body' + k);
        }
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['body0', 'disp0', 'sync-end', 'w1', 'body1', 'disp1', 'w2', 'after', 'w3', 'settled', 'w4'],
    'a head nested in a loop body disposes once per outer iteration'
  );

  log = await observe(function (L) {
    return (async function () {
      var fs = [];
      var i = 0;
      for (await using a = { tag: 1, [Symbol.asyncDispose]() { L('disp'); } }; i < 2; i++) {
        fs.push(function () { return a.tag; });
      }
      L('closures-' + fs.map(function (f) { return f(); }).join());
    })();
  });
  assert.compareArray(
    log,
    ['disp', 'sync-end', 'w1', 'closures-1,1', 'w2', 'settled', 'w3', 'w4'],
    'the const-like binding is shared by every iteration'
  );

  log = await observe(function (L) {
    return (async function () {
      var a = 'outer';
      for (await using a = { v: 'inner', [Symbol.asyncDispose]() { L('disp'); } }; false;) {}
      L('after-' + a);
    })();
  });
  assert.compareArray(
    log,
    ['disp', 'sync-end', 'w1', 'after-outer', 'w2', 'settled', 'w3', 'w4'],
    'the head binding does not leak into the enclosing scope'
  );
});
