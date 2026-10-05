// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-with-statement-runtime-semantics-evaluation
description: >
  Nested `with`s wrapping a directly-disposing `await using` block chain
  every enclosing with-environment correctly; a `with` whose expression
  isn't coercible to an object rejects with a TypeError from ToObject
  before the body (and its resource) is ever created; the try-block
  scope-opening call site is reached the same way the plain-block one is;
  and a throw out of a with-wrapped scope still disposes before
  propagating the original error (issue #858).
info: |
  WithStatement : with ( Expression ) Statement

  [...]
  3. Let obj be ? ToObject(val).
  4. Let newEnv be NewObjectEnvironment(obj, true, oldEnv).
  5. Set the running execution context's LexicalEnvironment to newEnv.
  6. Let C be Completion(Evaluation of Statement).
  [...]

  ToObject ( argument )

  Undefined, Null: Throw a TypeError exception.

  DisposeResources ( disposeCapability, completion )

  [...]
  3. For each element resource of disposeCapability.[[DisposableResourceStack]], in reverse list order, do
     [...]
  [...]
  6. Return ? completion.

  DisposeResources returns the completion it was given (after every
  disposer has run), so a throw that leaves a with-wrapped scope still
  disposes first and then propagates unchanged.
flags: [async, noStrict]
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
      with ({ tag: 'outer' })
      with ({ tag: 'inner' }) {
        await using x = { [Symbol.asyncDispose]() { L('dispose-' + tag); } };
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['dispose-inner', 'sync-end', 'w1', 'after', 'w2', 'settled', 'w3', 'w4'],
    'disposal of an await-using nested two with-levels deep (no intervening block) still suspends the function instead of draining inline, and the innermost with-object shadows the outer one'
  );
});

asyncTest(async function () {
  var log = [];
  var created = false;
  var caught;
  try {
    await (async function () {
      with (null) {
        created = true;
        await using a = { [Symbol.asyncDispose]() { log.push('dispose'); } };
      }
    })();
  } catch (e) {
    caught = e;
  }
  assert.sameValue(caught instanceof TypeError, true, 'with(null) rejects with a TypeError from ToObject');
  assert.sameValue(created, false, 'the with-body never runs, so the resource is never created');
  assert.compareArray(log, [], 'no disposal call: the resource was never created');
});

asyncTest(async function () {
  var log = [];
  await (async function () {
    with ({}) {
      try {
        await using a = { [Symbol.asyncDispose]() { log.push('dispose'); } };
        log.push('try-body');
      } finally {
        log.push('finally');
      }
    }
  })();
  assert.compareArray(
    log,
    ['try-body', 'dispose', 'finally'],
    'a with wrapping a try-block that directly declares await using disposes before the finally runs'
  );
});

asyncTest(async function () {
  var log = [];
  var caught;
  try {
    await (async function () {
      with ({}) {
        await using a = { [Symbol.asyncDispose]() { log.push('dispose'); } };
        throw new Test262Error('from-with-scope');
      }
    })();
  } catch (e) {
    caught = e;
  }
  assert.sameValue(caught && caught.message, 'from-with-scope', 'the original thrown error propagates unchanged');
  assert.compareArray(log, ['dispose'], 'the resource still disposes before the throw propagates');
});
