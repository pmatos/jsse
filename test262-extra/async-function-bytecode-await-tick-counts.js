// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-await
description: >
  An async function body hosted as a suspending bytecode chunk keeps the
  microtask tick count of every await: a primitive, a native promise, a
  pending promise, a thenable, a rejection, a promise whose constructor is
  read, and the return of a promise from an async function.
info: |
  Await ( value )

    2. Let promise be ? PromiseResolve(%Promise%, value).
    9. Perform PerformPromiseThen(promise, onFulfilled, onRejected).

  The order of the logged continuations is the order in which the reaction
  jobs run, which pins how many ticks each form takes.
flags: [async]
includes: [compareArray.js]
features: [async-functions]
---*/

var log = [];
var EXPECTED = [
  "prim:start",
  "pr:start",
  "pend:start",
  "ctor-read",
  "prim:after",
  "pr:after",
  "returnValue:resolved",
  "aa:4",
  "then-called",
  "rej:rej",
  "ctor:8",
  "T:tick1",
  "awaitPrimitive:resolved",
  "awaitPromise:resolved",
  "pend:after",
  "returnAwait:resolved",
  "awaitAsync:resolved",
  "thenable:7",
  "caught:tt",
  "constructorGetter:resolved",
  "rejectTop:top",
  "T:tick2",
  "awaitPending:resolved",
  "returnPromise:resolved",
  "aap:5",
  "thenableAwait:resolved",
  "thenableThrows:resolved",
  "rej2:rej2",
  "chain:done",
  "T:tick3",
  "awaitAwaitAsync:resolved",
  "rejectionPath:resolved",
  "chain:resolved",
  "T:tick4",
  "T:tick5",
  "T:tick6",
  "T:tick7",
  "T:tick8",
  "T:tick9",
  "T:tick10",
  "T:tick11",
  "T:tick12"
];
function tick(tag, n) {
  var p = Promise.resolve();
  for (var i = 1; i <= n; i++) {
    (function (i) { p = p.then(function () { log.push(tag + ":tick" + i); }); })(i);
  }
  return p;
}
async function awaitPrimitive() { log.push("prim:start"); await 1; log.push("prim:after"); }
async function awaitPromise() { log.push("pr:start"); await Promise.resolve(2); log.push("pr:after"); }
async function awaitPending() {
  var resolve; var p = new Promise(function (r) { resolve = r; });
  log.push("pend:start");
  Promise.resolve().then(function () { resolve(3); });
  await p;
  log.push("pend:after");
}
async function returnValue() { return 4; }
async function returnPromise() { return Promise.resolve(5); }
async function returnAwait() { return await Promise.resolve(6); }
async function awaitAsync() { var v = await returnValue(); log.push("aa:" + v); }
async function awaitAwaitAsync() { var v = await returnPromise(); log.push("aap:" + v); }
async function thenableAwait() {
  var thenable = { then: function (res) { log.push("then-called"); res(7); } };
  var v = await thenable;
  log.push("thenable:" + v);
}
async function thenableThrows() {
  var thenable = { then: function () { throw new Error("tt"); } };
  try { await thenable; } catch (e) { log.push("caught:" + e.message); }
}
async function rejectionPath() {
  try { await Promise.reject(new Error("rej")); log.push("unreachable"); } catch (e) { log.push("rej:" + e.message); }
  try { await { then(_, rej) { rej(new Error("rej2")); } }; } catch (e) { log.push("rej2:" + e.message); }
}
async function rejectTop() { await Promise.reject(new Error("top")); }
async function chain() { await 1; await 2; await 3; log.push("chain:done"); }
async function constructorGetter() {
  var p = Promise.resolve(8);
  Object.defineProperty(p, "constructor", { get: function () { log.push("ctor-read"); return Promise; } });
  var v = await p;
  log.push("ctor:" + v);
}
var tests = [awaitPrimitive, awaitPromise, awaitPending, returnValue, returnPromise, returnAwait, awaitAsync, awaitAwaitAsync, thenableAwait, thenableThrows, rejectionPath, chain, constructorGetter];
var results = [];
var order = 0;
tests.forEach(function (fn) {
  var tag = fn.name;
  var p = fn();
  p.then(function (v) { log.push(tag + ":resolved"); }, function (e) { log.push(tag + ":rejected:" + e.message); });
});
rejectTop().then(null, function (e) { log.push("rejectTop:" + e.message); });
tick("T", 12).then(function () {
  assert.compareArray(log, EXPECTED);
}).then($DONE, $DONE);
