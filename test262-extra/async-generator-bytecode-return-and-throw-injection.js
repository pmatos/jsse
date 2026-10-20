// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorunwrapyieldresumption
description: >
  yield and return await their operand; a return() argument is awaited before
  the generator unwinds, so a rejected argument throws at the yield and a
  finally block that awaits or yields completes before the request settles;
  a throw() at a yield is raised there; return() on a finished or unstarted
  generator awaits its argument.
info: |
  AsyncGeneratorYield ( value ) and AsyncGeneratorUnwrapYieldResumption
  await the operand of a yield and the value of a return resumption.
flags: [async]
includes: [compareArray.js]
features: [async-iteration]
---*/

var log = [];
var EXPECTED = [
  "y1={\"value\":\"p1\",\"done\":false}",
  "a=A",
  "caught:yield-rejected",
  "y2={\"value\":\"thenable\",\"done\":false}",
  "y3={\"value\":\"returned-promise\",\"done\":true}",
  "y4={\"done\":true}",
  "y5={\"done\":true}",
  "rr!return-rejected",
  "rt={\"value\":\"thenable-ret\",\"done\":true}",
  "fin-start",
  "fin-end",
  "f-ret={\"value\":\"R\",\"done\":true}",
  "fin-start",
  "fin-end",
  "f-throw!T",
  "fy-ret1={\"value\":\"from-finally\",\"done\":false}",
  "fin-after-yield",
  "fy-ret2={\"value\":\"R\",\"done\":true}",
  "cleanup",
  "c-ret={\"value\":\"X\",\"done\":true}",
  "f-inner",
  "f-outer",
  "n-ret={\"value\":\"Y\",\"done\":true}",
  "iterator-closed",
  "ci-ret={\"value\":\"Z\",\"done\":true}",
  "ri!return-arg-rejected",
  "tb1!body-throw",
  "tb2={\"done\":true}",
  "op1",
  "yielded",
  "op3",
  "r=1,sent,3",
  "oo={\"done\":true}"
];
function later(value) { return new Promise(function (resolve) { Promise.resolve().then(function () { resolve(value); }); }); }
function laterReject(msg) { return new Promise(function (_, reject) { Promise.resolve().then(function () { reject(new Error(msg)); }); }); }
function show(tag) { return function (r) { log.push(tag + "=" + JSON.stringify(r)); }; }
function fail(tag) { return function (e) { log.push(tag + "!" + (e && e.message !== undefined ? e.message : String(e))); }; }
function note(tag, v) { log.push(tag); return v; }

async function* yieldsPromises() {
  var a = yield later("p1");
  log.push("a=" + a);
  try { yield laterReject("yield-rejected"); log.push("not-reached"); } catch (e) { log.push("caught:" + e.message); }
  yield { then(res) { res("thenable"); } };
  return later("returned-promise");
}
async function* returnRejected() { return laterReject("return-rejected"); }
async function* returnThenable() { return { then(res) { res("thenable-ret"); } }; }
async function* finallyAwaits() {
  try { yield 1; yield 2; } finally { log.push("fin-start"); await later(0); log.push("fin-end"); }
}
async function* finallyYields() {
  try { yield 1; } finally { yield "from-finally"; log.push("fin-after-yield"); }
}
async function* catchesReturn() {
  try { yield 1; } catch (e) { log.push("never"); } finally { log.push("cleanup"); }
}
async function* nested() {
  try { try { yield "inner"; } finally { log.push("f-inner"); } } finally { log.push("f-outer"); }
}
async function* closesIterators() {
  var closed = [];
  var iterable = { [Symbol.iterator]() { return { next() { return { done: false, value: 1 }; }, return() { log.push("iterator-closed"); return {}; } }; } };
  for (var x of iterable) { yield x; }
}
async function* throwsInBody() { yield 1; throw new Error("body-throw"); }
async function* operandOrder() {
  var r = [note("op1", 1), yield note("yielded", 2), note("op3", 3)];
  log.push("r=" + r.join());
}

(async function () {
  var g = yieldsPromises();
  show("y1")(await g.next());
  show("y2")(await g.next("A"));
  show("y3")(await g.next());
  show("y4")(await g.next());
  show("y5")(await g.next());

  await returnRejected().next().then(show("rr"), fail("rr"));
  show("rt")(await returnThenable().next());

  var f = finallyAwaits(); await f.next();
  show("f-ret")(await f.return("R"));
  var f2 = finallyAwaits(); await f2.next();
  await f2.throw(new Error("T")).then(show("f-throw"), fail("f-throw"));

  var fy = finallyYields(); await fy.next();
  show("fy-ret1")(await fy.return("R"));
  show("fy-ret2")(await fy.next());

  var c = catchesReturn(); await c.next();
  show("c-ret")(await c.return("X"));
  var n = nested(); await n.next();
  show("n-ret")(await n.return(later("Y")));
  var ci = closesIterators(); await ci.next();
  show("ci-ret")(await ci.return("Z"));
  var ri = yieldsPromises(); await ri.next();
  await ri.return(laterReject("return-arg-rejected")).then(show("ri"), fail("ri"));
  var tb = throwsInBody(); await tb.next();
  await tb.next().then(show("tb1"), fail("tb1"));
  show("tb2")(await tb.next());
  var oo = operandOrder(); await oo.next(); show("oo")(await oo.next("sent"));
})().then(function () {
  assert.compareArray(log, EXPECTED);
}).then($DONE, $DONE);
