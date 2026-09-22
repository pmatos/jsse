// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-return-statement-runtime-semantics-evaluation
description: >
  `return;` in an async generator has no operand, so no Await precedes the
  return completion: the enclosing `finally` runs in the job that resumed the
  generator, ahead of jobs that were already queued. `return expr;` awaits
  `expr` first, so its `finally` runs one job later.
info: |
  ReturnStatement : return ;

  1. Return Completion Record { [[Type]]: return, [[Value]]: undefined, [[Target]]: empty }.

  ReturnStatement : return Expression ;

  3. If GetGeneratorKind() is async, set exprValue to ? Await(exprValue).
  4. Return Completion Record { [[Type]]: return, [[Value]]: exprValue, [[Target]]: empty }.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

async function observe(makeGen) {
  var log = [];
  var L = function (entry) { log.push(entry); };
  var it = makeGen(L);
  await it.next();
  var p = Promise.resolve();
  for (var i = 1; i <= 5; i++) {
    (function (n) { p = p.then(function () { L('w' + n); }); })(i);
  }
  var settled = it.next();
  await settled;
  L('settled');
  return log.slice();
}

asyncTest(async function () {
  var log = await observe(function (L) {
    return (async function* () {
      try {
        yield 0;
        return;
      } finally {
        L('fin');
      }
    })();
  });
  assert.compareArray(
    log,
    ['fin', 'w1', 'settled'],
    '`return;` runs the finally synchronously'
  );

  log = await observe(function (L) {
    return (async function* () {
      try {
        yield 0;
        return 5;
      } finally {
        L('fin');
      }
    })();
  });
  assert.compareArray(
    log,
    ['w1', 'fin', 'w2', 'settled'],
    '`return expr;` awaits the operand before the finally runs'
  );
});
