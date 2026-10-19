// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-try-statement-runtime-semantics-evaluation
description: >
  try/catch/finally around an await inside an async function: a rejection is
  thrown at the await, a finally that awaits completes before the pending
  completion is applied, a return or throw in a finally replaces the pending
  one, and loop control crosses awaiting finally blocks and for-of iterator
  closing.
info: |
  TryStatement : try Block Catch Finally

  The Finally block runs for every completion of the try and catch blocks, may
  Await, and its own abrupt completion replaces the pending one.
flags: [async]
includes: [compareArray.js]
features: [async-functions]
---*/

var log = [];
var EXPECTED = [
  "caught:a",
  "catchRejection=>ret-from-catch",
  "fin1",
  "fin1-after",
  "finallyRunsAfterAwaitedReturn=>r1",
  "finallyOverridesReturn=>outer",
  "finallyOverridesThrow=>swallowed",
  "throwInFinallyReplaces!!second",
  "nestedTry=>f-inner,c:inner,c2:again,f2,f-outer",
  "loopWithTry=>body0,fin0,fin1,body2,fin2,fin3",
  "breakOutOfLabeledAcrossFinally=>00,f00,f01,10,f10,f11,f20",
  "forOfCloseOnAwaitedThrow=>closed,caught:stop",
  "catch-x:thrown",
  "outer-x:outer",
  "awaitInCatchParamScope=>undefined",
  "rejectedAwaitUnwindsThroughHandlers=>f1,deep",
  "uncaught!!uncaught",
  "fin-through",
  "uncaughtThroughFinally!!through"
];
function later(value) { return new Promise(function (resolve) { Promise.resolve().then(function () { resolve(value); }); }); }
function laterReject(msg) { return new Promise(function (_, reject) { Promise.resolve().then(function () { reject(new Error(msg)); }); }); }

async function catchRejection() {
  try { await laterReject("a"); log.push("no"); } catch (e) { log.push("caught:" + e.message); return await later("ret-from-catch"); }
}
async function finallyRunsAfterAwaitedReturn() {
  try { return await later("r1"); } finally { log.push("fin1"); await later(0); log.push("fin1-after"); }
}
async function finallyOverridesReturn() {
  try { return "inner"; } finally { await later(0); return "outer"; }
}
async function finallyOverridesThrow() {
  try { throw new Error("lost"); } finally { await later(0); return "swallowed"; }
}
async function throwInFinallyReplaces() {
  try { throw new Error("first"); } finally { await later(0); throw new Error("second"); }
}
async function nestedTry() {
  var out = [];
  try {
    try { await laterReject("inner"); } finally { out.push("f-inner"); await later(0); }
  } catch (e) {
    out.push("c:" + e.message);
    try { await later(0); throw new Error("again"); } catch (e2) { out.push("c2:" + e2.message); } finally { out.push("f2"); }
  } finally { out.push("f-outer"); }
  return out.join(",");
}
async function loopWithTry() {
  var out = [];
  for (var i = 0; i < 4; i++) {
    try {
      if (i === 1) continue;
      if (i === 3) break;
      await later(i);
      out.push("body" + i);
    } finally { out.push("fin" + i); await later(0); }
  }
  return out.join(",");
}
async function breakOutOfLabeledAcrossFinally() {
  var out = [];
  outer: for (var i = 0; i < 3; i++) {
    for (var j = 0; j < 3; j++) {
      try { await later(0); if (j === 1) continue outer; if (i === 2) break outer; out.push(i + "" + j); }
      finally { out.push("f" + i + j); }
    }
  }
  return out.join(",");
}
async function forOfCloseOnAwaitedThrow() {
  var closed = [];
  var iterable = { [Symbol.iterator]() { var n = 0; return { next() { return { done: n > 2, value: n++ }; }, return() { closed.push("closed"); return {}; } }; } };
  try {
    for (var v of iterable) { await later(v); if (v === 1) throw new Error("stop"); }
  } catch (e) { closed.push("caught:" + e.message); }
  return closed.join(",");
}
async function awaitInCatchParamScope() {
  var x = "outer";
  try { throw "thrown"; } catch (x) { await later(0); log.push("catch-x:" + x); }
  log.push("outer-x:" + x);
}
async function rejectedAwaitUnwindsThroughHandlers() {
  var out = [];
  try { try { await laterReject("deep"); } finally { out.push("f1"); } } catch (e) { out.push(e.message); }
  return out.join(",");
}
async function uncaught() { await later(0); throw new Error("uncaught"); }
async function uncaughtThroughFinally() { try { await later(0); throw new Error("through"); } finally { log.push("fin-through"); } }
var cases = [catchRejection, finallyRunsAfterAwaitedReturn, finallyOverridesReturn, finallyOverridesThrow, throwInFinallyReplaces, nestedTry, loopWithTry, breakOutOfLabeledAcrossFinally, forOfCloseOnAwaitedThrow, awaitInCatchParamScope, rejectedAwaitUnwindsThroughHandlers, uncaught, uncaughtThroughFinally];
(async function () {
  for (var i = 0; i < cases.length; i++) {
    var fn = cases[i];
    try { log.push(fn.name + "=>" + await fn()); } catch (e) { log.push(fn.name + "!!" + e.message); }
  }
})().then(function () {
  assert.compareArray(log, EXPECTED);
}).then($DONE, $DONE);
