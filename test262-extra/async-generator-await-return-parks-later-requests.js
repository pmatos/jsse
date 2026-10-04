// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorawaitreturn
description: >
  While AsyncGeneratorAwaitReturn waits for its operand, the generator is in
  the draining-queue state: later requests wait behind it and are served, in
  order, by AsyncGeneratorDrainQueue once the operand settles.
info: |
  AsyncGeneratorAwaitReturn ( generator )

  [...]
  9. Let onFulfilled be CreateBuiltinFunction(fulfilledClosure, 1, "", « »).
  [...]
  13. Perform PerformPromiseThen(promise, onFulfilled, onRejected).

  fulfilledClosure performs AsyncGeneratorCompleteStep and then
  AsyncGeneratorDrainQueue; until then the request stays at the head of the
  queue, so a request made meanwhile is only enqueued (%AsyncGeneratorPrototype%.next
  step 8, .return step 9, .throw step 9).
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration, async-explicit-resource-management]
---*/

function flush() {
  var p = Promise.resolve();
  for (var i = 0; i < 10; i++) p = p.then(function () {});
  return p;
}

function inner(next, ret) {
  var it = {
    next: next || function () { return { value: 1, done: false }; },
    return: ret
  };
  it[Symbol.asyncIterator] = function () { return this; };
  return it;
}

var starts = {
  'suspended-start': [function () { return (async function* () {})(); }, null],
  completed: [function () { return (async function* () {})(); }, function (it) { return it.next(); }],
  'suspended-yield': [function () { return (async function* () { yield 1; })(); }, function (it) { return it.next(); }],
  'yield in try-finally': [function () { return (async function* () { try { yield 1; } finally {} })(); }, function (it) { return it.next(); }],
  'yield* with return() completing the delegation': [function () {
    return (async function* () { yield* inner(null, function (v) { return { value: v, done: true }; }); })();
  }, function (it) { return it.next(); }],
  'yield* without return()': [function () { return (async function* () { yield* inner(); })(); }, function (it) { return it.next(); }],
  'await using at the return': [function () {
    return (async function* () { await using d = { async [Symbol.asyncDispose]() {} }; yield 1; })();
  }, function (it) { return it.next(); }]
};

var followers = {
  next: ['next', 'next:undefined:true'],
  throw: ['throw', 'throw-rejected:x'],
  return: ['return', 'return:x:true']
};

asyncTest(async function () {
  for (var name in starts) {
    for (var kind in followers) {
      var it = starts[name][0]();
      if (starts[name][1]) await starts[name][1](it);
      await flush();

      var release;
      var pending = new Promise(function (resolve) { release = resolve; });
      var log = [];
      var first = it.return(pending).then(function (r) { log.push('first:' + r.value + ':' + r.done); });
      var second = it[kind]('x').then(
        function (r) { log.push(kind + ':' + r.value + ':' + r.done); },
        function (e) { log.push(kind + '-rejected:' + e); }
      );

      await flush();
      assert.compareArray(log, [], name + ' / ' + kind + ': nothing settles while the operand is pending');

      release(5);
      await Promise.all([first, second]);
      assert.compareArray(
        log,
        ['first:5:true', kind === 'next' ? 'next:undefined:true' : kind === 'throw' ? 'throw-rejected:x' : 'return:x:true'],
        name + ' / ' + kind + ': the return settles first, then the later request'
      );
    }
  }
});
