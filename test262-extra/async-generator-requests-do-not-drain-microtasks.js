// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorcompletestep
description: >
  Settling an async generator request only enqueues the reactions of its
  promise. A job queued before next/return/throw was called does not run inside
  the call, whichever path settles the request.
info: |
  AsyncGeneratorCompleteStep ( generator, completion, done [ , realm ] )

  [...]
  8. Perform ! Call(promiseCapability.[[Resolve]], undefined, « iteratorResult »).
  9. Return unused.

  Calling a promise capability's resolve or reject function enqueues jobs; it
  never runs them. The request methods return the promise to their caller
  before any job runs.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration, async-explicit-resource-management]
---*/

function flush() {
  var p = Promise.resolve();
  for (var i = 0; i < 12; i++) p = p.then(function () {});
  return p;
}
function boom(v) { return function () { throw v; }; }
var badIterable = {};
badIterable[Symbol.iterator] = boom('iterable');
var laterBad = {};
laterBad[Symbol.iterator] = function () {
  var n = 0;
  return { next: function () { if (n++) throw 'next'; return { value: 1, done: false }; } };
};
function delegate(methods) {
  var it = { next: methods.next || function () { return { value: 1, done: false }; } };
  if (methods.return) it.return = methods.return;
  it[Symbol.asyncIterator] = function () { return this; };
  return it;
}
function first(it) { return it.next(); }

var scenarios = {
  'next: body throws': [async function* () { throw 'E'; }, null, function (it) { return it.next(); }],
  'next: completed': [async function* () {}, first, function (it) { return it.next(); }],
  'next: resumes into a throw': [async function* () { yield 1; throw 'E'; }, first, function (it) { return it.next(); }],
  'next: try/finally throw': [async function* () { try { yield 1; throw 'E'; } finally {} }, first, function (it) { return it.next(); }],
  'next: try/catch rethrow': [async function* () { try { yield 1; throw 'E'; } catch (e) { throw 'R'; } }, first, function (it) { return it.next(); }],
  'next: while condition throws': [async function* () { var i = 0; while (i++ < 1 || boom('E')()) { yield 1; } }, first, function (it) { return it.next(); }],
  'next: switch discriminant throws': [async function* () { switch (boom('E')()) { case 1: yield 1; } }, null, function (it) { return it.next(); }],
  'next: switch case test throws': [async function* () { switch (1) { case boom('E')(): yield 1; } }, null, function (it) { return it.next(); }],
  'next: for-of iterable throws': [async function* () { for (var x of badIterable) { yield x; } }, null, function (it) { return it.next(); }],
  'next: for-of step throws': [async function* () { for (var x of laterBad) { yield x; } }, first, function (it) { return it.next(); }],
  'next: yield* next() throws': [async function* () { yield* delegate({ next: boom('E') }); }, null, function (it) { return it.next(); }],
  'next: yield* result is not an object': [async function* () { yield* delegate({ next: function () { return 1; } }); }, null, function (it) { return it.next(); }],
  'return: suspended-start': [async function* () {}, null, function (it) { return it.return(1); }],
  'return: suspended-start, rejected promise': [async function* () {}, null, function (it) { return it.return(Promise.reject('E')); }],
  'return: suspended-start, thenable': [async function* () {}, null, function (it) { return it.return({ then: function (r) { r(1); } }); }],
  'return: completed': [async function* () {}, first, function (it) { return it.return(1); }],
  'return: suspended-yield': [async function* () { yield 1; }, first, function (it) { return it.return(1); }],
  'return: suspended-yield in try/finally': [async function* () { try { yield 1; } finally {} }, first, function (it) { return it.return(1); }],
  'return: await using disposer throws': [async function* () { await using d = { async [Symbol.asyncDispose]() { throw 'D'; } }; yield 1; }, first, function (it) { return it.return(1); }],
  'return: yield* return() throws': [async function* () { yield* delegate({ return: boom('E') }); }, first, function (it) { return it.return(1); }],
  'throw: suspended-yield': [async function* () { yield 1; }, first, function (it) { return it.throw('E'); }],
  'throw: completed': [async function* () {}, first, function (it) { return it.throw('E'); }],
  'throw: await using disposer throws': [async function* () { await using d = { async [Symbol.asyncDispose]() { throw 'D'; } }; yield 1; }, first, function (it) { return it.throw('T'); }],
  'throw: yield* has no throw()': [async function* () { yield* delegate({}); }, first, function (it) { return it.throw('T'); }]
};

asyncTest(async function () {
  for (var name in scenarios) {
    var it = scenarios[name][0]();
    if (scenarios[name][1]) await scenarios[name][1](it);
    await flush();

    var log = [];
    Promise.resolve().then(function () { log.push('job'); });
    var request = scenarios[name][2](it);
    log.push('returned');
    request.then(function () {}, function () {});
    await flush();
    assert.compareArray(log, ['returned', 'job'], name);
  }
});
