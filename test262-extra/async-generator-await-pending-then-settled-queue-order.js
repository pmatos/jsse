// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorenqueue
description: >
  One async generator that awaits and yields operands of every kind (pending
  promise, already settled promise, non-object, thenable, rejected promise)
  while next/return/throw requests are queued behind the running one. Each
  await, yield and return-await costs exactly the same number of jobs as the
  matching promise reaction, so the interleaving with an independent tick
  counter is fixed.
info: |
  Await ( value )

  [...]
  2. Let promise be ? PromiseResolve(%Promise%, value).
  [...]
  7. Perform PerformPromiseThen(promise, onFulfilled, onRejected).

  AsyncGeneratorYield ( value )

  [...]
  5. Perform AsyncGeneratorCompleteStep(generator, completion, false, previousRealm).
  6. Let queue be generator.[[AsyncGeneratorQueue]].
  7. If queue is not empty, then
     a. NOTE: Execution continues without suspending the generator.
     b. Let toYield be the first element of queue.
     c. Let resumptionValue be Completion(toYield.[[Completion]]).
     d. Return ? AsyncGeneratorUnwrapYieldResumption(resumptionValue).

  The reaction jobs of the awaits and of the request promises are observable
  only through their order relative to each other and to an independent tick
  counter.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

var log = [];
var tick = 0;
function ticker() {
  tick++;
  if (tick <= 40) Promise.resolve().then(ticker);
}
function mark(label) { log.push(label + '@' + tick); }

var release;
var gate = new Promise(function (resolve) { release = resolve; });
var thenable = { then: function (resolve) { mark('thenable.then'); resolve('T'); } };

async function* g() {
  mark('start');
  var a = await gate;
  mark('after pending ' + a);
  var b = await Promise.resolve('settled');
  mark('after settled ' + b);
  var c = await 42;
  mark('after non-object ' + c);
  var d = await thenable;
  mark('after thenable ' + d);
  try {
    await Promise.reject('R');
  } catch (e) {
    mark('caught ' + e);
  }
  var e = yield gate;
  mark('resumed after yield pending ' + e);
  var f = yield Promise.resolve('Y2');
  mark('resumed after yield settled ' + f);
  return 'done';
}

async function* rejecting() {
  yield 'first';
  yield Promise.reject('YR');
  mark('unreachable');
}

asyncTest(async function () {
  var it = g();
  Promise.resolve().then(ticker);
  var results = [
    it.next('n1').then(function (r) { mark('next1 ' + r.value + '/' + r.done); }),
    it.next('n2').then(function (r) { mark('next2 ' + r.value + '/' + r.done); }),
    it.next('n3').then(function (r) { mark('next3 ' + r.value + '/' + r.done); }),
    it.next('n4').then(function (r) { mark('next4 ' + r.value + '/' + r.done); }),
    it.return('R5').then(function (r) { mark('return5 ' + r.value + '/' + r.done); }),
    it.next('n6').then(function (r) { mark('next6 ' + r.value + '/' + r.done); }),
    it.throw('T7').then(function () {}, function (e) { mark('throw7 ' + e); })
  ];
  Promise.resolve().then(function () { mark('gate release'); release('G'); });
  await Promise.all(results);
  log.push('end@' + tick);

  var r = rejecting();
  var more = [
    r.next().then(function (v) { mark('r1 ' + v.value + '/' + v.done); }),
    r.next().then(function () {}, function (e) { mark('r2 rejected ' + e); }),
    r.next().then(function (v) { mark('r3 ' + v.value + '/' + v.done); }),
    r.next().then(function (v) { mark('r4 ' + v.value + '/' + v.done); })
  ];
  await Promise.all(more);
  log.push('end2@' + tick);

  assert.compareArray(log, [
  'start@0',
  'gate release@1',
  'after pending G@2',
  'after settled settled@3',
  'after non-object 42@4',
  'thenable.then@5',
  'after thenable T@6',
  'caught R@7',
  'resumed after yield pending n2@8',
  'next1 G/false@9',
  'resumed after yield settled n3@9',
  'next2 Y2/false@10',
  'next3 done/true@11',
  'next4 undefined/true@11',
  'return5 R5/true@12',
  'next6 undefined/true@12',
  'throw7 T7@12',
  'end@14',
  'r1 first/false@16',
  'r2 rejected YR@17',
  'r3 undefined/true@17',
  'r4 undefined/true@17',
  'end2@19'
], log.join(' | '));
});
