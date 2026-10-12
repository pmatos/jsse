/*---
description: >
  `await` queues exactly one continuation job for a non-object, a native
  promise (settled or pending, fulfilled or rejected) and reads
  `value.constructor` exactly once; a thenable or a promise with a foreign
  constructor takes the extra PromiseResolveThenableJob ticks.
esid: await
info: |
  Await ( value )
    2. Let promise be ? PromiseResolve(%Promise%, value).
    9. Perform PerformPromiseThen(promise, onFulfilled, onRejected).

  PromiseResolve ( C, x )
    1. If IsPromise(x) is true, then
      a. Let xConstructor be ? Get(x, "constructor").
      b. If SameValue(xConstructor, C) is true, return x.
    2. Let promiseCapability be ? NewPromiseCapability(C).
    3. Perform ? Call(promiseCapability.[[Resolve]], undefined, « x »).
flags: [async]
---*/

var log = [];
function mark(name) { log.push(name); }

var constructorReads = 0;
var observed = Promise.resolve("observed");
Object.defineProperty(observed, "constructor", {
  get: function () {
    constructorReads++;
    return Promise;
  }
});

var foreign = Promise.resolve("foreign");
foreign.constructor = function Foreign() {};

var pendingRelease;
var pending = new Promise(function (resolve) { pendingRelease = resolve; });

async function primitive() {
  await 1;
  mark("primitive:1");
  await undefined;
  mark("primitive:2");
}

async function nativeSettled() {
  await Promise.resolve(1);
  mark("native:1");
  await Promise.resolve(2);
  mark("native:2");
}

async function nativeRejected() {
  try {
    await Promise.reject(new Error("boom"));
  } catch (e) {
    mark("rejected:caught:" + e.message);
  }
  await null;
  mark("rejected:after");
}

async function thenable() {
  await { then: function (resolve) { mark("thenable:then"); resolve(1); } };
  mark("thenable:1");
}

async function observedConstructor() {
  var v = await observed;
  mark("observed:" + v);
}

async function foreignConstructor() {
  var v = await foreign;
  mark("foreign:" + v);
}

async function waitsOnPending() {
  var v = await pending;
  mark("pending:" + v);
}

primitive();
nativeSettled();
nativeRejected();
thenable();
observedConstructor();
foreignConstructor();
waitsOnPending();

var chain = Promise.resolve();
for (var i = 1; i <= 6; i++) {
  (function (n) {
    chain = chain.then(function () {
      mark("tick:" + n);
      if (n === 1) pendingRelease("late");
    });
  })(i);
}

chain.then(function () {
  assert.sameValue(constructorReads, 1, "Get(x, 'constructor') runs exactly once");
  assert.compareArray(log, [
    "primitive:1",
    "native:1",
    "rejected:caught:boom",
    "thenable:then",
    "observed:observed",
    "tick:1",
    "primitive:2",
    "native:2",
    "rejected:after",
    "thenable:1",
    "pending:late",
    "tick:2",
    "foreign:foreign",
    "tick:3",
    "tick:4",
    "tick:5",
    "tick:6"
  ]);
}).then($DONE, $DONE);
