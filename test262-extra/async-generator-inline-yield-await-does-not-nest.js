// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-yield
description: >
  A `yield` inside an expression the state-machine transform does not lower
  (here a destructuring-assignment default) suspends the async generator at
  `Await(value)` like a statement-position `yield`: jobs queued before the
  request are not drained inside it, and the request settles after the same
  number of ticks as the lowered form.
info: |
  Yield ( value )

  1. Let generatorKind be GetGeneratorKind().
  2. If generatorKind is async, return ? AsyncGeneratorYield(? Await(value)).

  Await ( value )

  Await suspends the running execution context; the continuation runs as its
  own job via PerformPromiseThen and never runs other jobs inline.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration, destructuring-assignment]
---*/

async function* inlineYield() { var a; ({a = yield 1} = {}); }
async function* loweredYield() { var a; yield 1; }

async function trace(makeGenerator) {
  var log = [];
  var it = makeGenerator();
  var chain = Promise.resolve();
  for (var i = 1; i <= 6; i++) {
    (function (n) { chain = chain.then(function () { log.push('w' + n); }); })(i);
  }
  var first = it.next().then(function (r) {
    log.push('next-resolved:' + r.value);
    return r;
  });
  log.push('after-next');
  await first;
  await chain;
  return log;
}

asyncTest(async function () {
  var inlineLog = await trace(inlineYield);
  var loweredLog = await trace(loweredYield);
  assert.sameValue(inlineLog[0], 'after-next', 'the request does not run queued jobs before returning');
  assert.compareArray(inlineLog, loweredLog, 'inline yield settles on the same ticks as a lowered yield');
});
