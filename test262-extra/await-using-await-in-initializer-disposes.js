// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-let-and-const-declarations-runtime-semantics-evaluation
description: >
  A `using` / `await using` declaration whose initializer contains an `await`
  still registers its resource, binds in the enclosing block's own scope, and
  is disposed when that scope exits.
info: |
  LexicalBinding : BindingIdentifier Initializer

  [...]
  3. Let rhs be ? Evaluation of Initializer.
  4. Let value be ? GetValue(rhs).
  5. If environment is not undefined, then
     a. Perform ? InitializeReferencedBinding(lhs, value).
  6. [...]
  7. If IsUsingDeclaration, then
     a. Perform ? AddDisposableResource(...).

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
  return { [Symbol.asyncDispose]() { L('disp' + name); } };
}

asyncTest(async function () {
  var log;

  log = await observe(function (L) {
    return (async function () {
      { await using a = await D(L, 'A'); L('body'); }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end','w1','body','dispA','w2','after','w3','settled','w4'],
    'a block-scoped declaration whose initializer awaits is still registered and disposed'
  );

  log = await observe(function (L) {
    return (async function () {
      await using a = await D(L, 'A');
      L('body');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end','w1','body','dispA','w2','w3','settled','w4'],
    'a function-level declaration whose initializer awaits is disposed at function exit'
  );

  log = await observe(function (L) {
    return (async function () {
      var i = 0;
      for (await using a = await D(L, 'A'); i < 1; i++) {
        L('body');
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end','w1','body','dispA','w2','after','w3','settled','w4'],
    'a for head whose initializer awaits disposes at loop exit'
  );

  log = await observe(function (L) {
    return (async function () {
      var a = 'outer';
      { await using a = await D(L, 'A'); L('body'); }
      L('after-' + a);
    })();
  });
  assert.compareArray(
    log,
    ['sync-end','w1','body','dispA','w2','after-outer','w3','settled','w4'],
    'the declaration binds in its own block and leaves an outer binding untouched'
  );

  log = await observe(function (L) {
    return (async function () {
      {
        await using a = await D(L, 'A'), b = await D(L, 'B');
        L('body');
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['sync-end','w1','w2','body','dispB','w3','dispA','w4','after','settled'],
    'declarators with awaiting initializers dispose in reverse order'
  );

  log = await observe(function (L) {
    return (async function () {
      try {
        { await using a = await a; }
      } catch (e) {
        L(e.constructor.name);
      }
      L('after');
    })();
  });
  assert.compareArray(
    log,
    ['ReferenceError','after','sync-end','w1','settled','w2','w3','w4'],
    'the binding is in its temporal dead zone while its own initializer runs'
  );

  log = await observe(function (L) {
    return (async function () {
      try {
        {
          await using a = await D(L, 'A'), b = await Promise.reject(new Error('r'));
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
    ['sync-end','w1','w2','dispA','w3','caught-r','after','w4','settled'],
    'a rejecting later initializer disposes the earlier declarators first'
  );

  log = await observe(function (L) {
    return (async function () {
      var f;
      { await using a = await D(L, 'A'); f = function () { return a; }; }
      L('after-' + typeof f());
    })();
  });
  assert.compareArray(
    log,
    ['sync-end','w1','dispA','w2','after-object','w3','settled','w4'],
    'a closure keeps the block binding alive after disposal'
  );
});
