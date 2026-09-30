// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-return-statement-runtime-semantics-evaluation
description: >
  `return expr;` in an async generator performs Await(expr) before the return
  completion unwinds, so a function-level async disposer starts only after the
  operand's Await, not synchronously inside the resuming `next()`.
info: |
  ReturnStatement : return Expression ;

  1. Let exprRef be ? Evaluation of Expression.
  2. Let exprValue be ? GetValue(exprRef).
  3. If GetGeneratorKind() is async, set exprValue to ? Await(exprValue).
  4. Return Completion Record { [[Type]]: return, [[Value]]: exprValue, [[Target]]: empty }.

  DisposeResources for the function body runs on that return completion
  (proposal-explicit-resource-management, sec-disposeresources).
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration]
---*/

asyncTest(async function () {
  var log = [];
  var L = function (entry) { log.push(entry); };
  var it = (async function* () {
    await using a = { async [Symbol.asyncDispose]() { L('d-start'); await null; L('d-end'); } };
    yield 1;
    L('before-return');
    return 5;
  })();
  await it.next();
  var second = it.next();
  second.then(function (r) { L('n2:' + r.value + ':' + r.done); });
  L('sync-end');
  await second;
  assert.compareArray(
    log,
    ['before-return', 'sync-end', 'd-start', 'd-end', 'n2:5:true'],
    'the disposer starts after the return operand is awaited'
  );

  log = [];
  var it2 = (async function* () {
    await using a = { async [Symbol.asyncDispose]() { L('d-start'); await null; L('d-end'); } };
    yield 1;
    return Promise.reject('operand-rejected');
  })();
  await it2.next();
  var rejected = it2.next();
  rejected.then(
    function () { L('resolved'); },
    function (e) { L('rejected:' + e); }
  );
  L('sync-end');
  try { await rejected; } catch (e) {}
  assert.compareArray(
    log,
    ['sync-end', 'd-start', 'd-end', 'rejected:operand-rejected'],
    'a rejected return operand is a throw completion that is disposed and then delivered'
  );
});
