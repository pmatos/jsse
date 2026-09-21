// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorunwrapyieldresumption
description: >
  `.return(v)` at a yield first Awaits `v`, and only then resumes the
  generator with the return completion: a `finally` around the yield does not
  run until the operand has settled, and the request resolves with the awaited
  value.
info: |
  AsyncGeneratorUnwrapYieldResumption ( resumptionValue )

  1. If resumptionValue is not a return completion, return ? resumptionValue.
  2. Let awaited be Completion(Await(resumptionValue.[[Value]])).
  3. If awaited is a throw completion, return ? awaited.
  4. Assert: awaited is a normal completion.
  5. Return Completion Record { [[Type]]: return, [[Value]]: awaited.[[Value]], [[Target]]: empty }.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

function drainTicks(n) {
  var p = Promise.resolve();
  for (var i = 0; i < n; i++) { p = p.then(function () {}); }
  return p;
}

asyncTest(async function () {
  var log = [];
  var L = function (entry) { log.push(entry); };

  var it = (async function* () {
    try { yield 1; } finally { L('fin'); }
  })();
  await it.next();
  var ret = it.return({ then(resolve) { L('then'); resolve(7); } });
  ret.then(function (r) { L('ret:' + r.value + ':' + r.done); });
  L('sync');
  await drainTicks(12);
  assert.compareArray(
    log,
    ['sync', 'then', 'fin', 'ret:7:true'],
    'the thenable operand is unwrapped before the finally runs'
  );

  log = [];
  var release;
  var pending = new Promise(function (resolve) { release = resolve; });
  it = (async function* () {
    try { yield 1; } finally { L('fin'); }
  })();
  await it.next();
  ret = it.return(pending);
  ret.then(function (r) { L('ret:' + r.value + ':' + r.done); });
  var nxt = it.next();
  nxt.then(function (r) { L('next:' + r.value + ':' + r.done); });
  await drainTicks(12);
  assert.compareArray(log, [], 'a pending operand keeps the finally from running');
  release(8);
  await ret;
  await nxt;
  assert.compareArray(
    log,
    ['fin', 'ret:8:true', 'next:undefined:true'],
    'the finally runs after the operand settles; the queued next stays behind the return'
  );

  log = [];
  it = (async function* () { yield 1; })();
  await it.next();
  var r = await it.return(Promise.resolve(3));
  assert.sameValue(r.value, 3, 'a resolved promise operand is unwrapped');
  assert.sameValue(r.done, true, 'the generator completes');

  log = [];
  it = (async function* () {
    try { yield 1; } finally { L('fin'); yield 'from-finally'; }
  })();
  await it.next();
  r = await it.return(Promise.resolve('v'));
  assert.sameValue(r.value, 'from-finally', 'a yield in the finally suspends the return');
  assert.sameValue(r.done, false, 'the generator is not done at the yield in finally');
  r = await it.next();
  assert.sameValue(r.value, 'v', 'the return completes with the awaited operand once the finally ends');
  assert.sameValue(r.done, true, 'and the generator is done');
});
