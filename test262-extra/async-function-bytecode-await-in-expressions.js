// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-async-function-definitions-runtime-semantics-evaluation
description: >
  Operands that precede an await are evaluated before the function suspends
  and keep their values across it: call arguments, callee and receiver,
  binary operands, assignment targets and keys, compound and logical
  assignments, templates, array and object literals, update and delete
  operands, conditions of loops, if and switch, new, and sequences.
info: |
  EvaluateCall and the operator semantics evaluate their operands left to
  right; an Await in a later operand suspends after the earlier ones ran.
flags: [async]
includes: [compareArray.js]
features: [async-functions]
---*/

var log = [];
var EXPECTED = [
  "--operandsBeforeAwait",
  "arg1",
  "await-arg",
  "arg3",
  "call:1,2,3",
  "--calleeAndThis",
  "callee-base",
  "method:o9",
  "--binaryLeftFirst",
  "left",
  "right",
  "binary:15",
  "--memberAssign",
  "target",
  "key",
  "value",
  "assign:42",
  "--compoundAssign",
  "compound:11,15",
  "--logical",
  "logical:nullish|or|and|then",
  "--templateAndSpread",
  "t2",
  "spread",
  "ck",
  "tmpl:a1b2c3 arr:1,2,3,4 obj:{\"k\":\"v\",\"c\":\"w\"}",
  "--updateAndDelete",
  "upd:{\"a\":2}",
  "--awaitInArguments",
  "args:7 x-y",
  "--awaitInCondition",
  "cond:2126",
  "--awaitStatementForms",
  "stmt:3,9",
  "--awaitInNew",
  "new:boxed,cls",
  "--sequenceAndComma",
  "s1",
  "s3",
  "seq:s4"
];
function note(tag, value) { log.push(tag); return value; }
function tick(tag, n) {
  var p = Promise.resolve();
  for (var i = 1; i <= n; i++) {
    (function (i) { p = p.then(function () { log.push(tag + ":tick" + i); }); })(i);
  }
  return p;
}
function later(value) { return new Promise(function (resolve) { Promise.resolve().then(function () { resolve(value); }); }); }
function add3(a, b, c) { return a + "," + b + "," + c; }

async function operandsBeforeAwait() {
  var r = add3(note("arg1", 1), await later(note("await-arg", 2)), note("arg3", 3));
  log.push("call:" + r);
}
async function calleeAndThis() {
  var obj = { name: "o", m: function (x) { return this.name + x; } };
  var r = note("callee-base", obj).m(await later(9));
  log.push("method:" + r);
}
async function binaryLeftFirst() {
  var r = note("left", 10) + await later(note("right", 5));
  log.push("binary:" + r);
}
async function memberAssign() {
  var o = { a: 1 };
  note("target", o)[note("key", "k")] = await later(note("value", 42));
  log.push("assign:" + o.k);
}
async function compoundAssign() {
  var o = { n: 1 };
  o.n += await later(10);
  var x = 5;
  x *= await later(3);
  log.push("compound:" + o.n + "," + x);
}
async function logical() {
  var a = null, b = 0, c = 1;
  var r1 = a ?? await later("nullish");
  var r2 = b || await later("or");
  var r3 = c && await later("and");
  var r4 = c ? await later("then") : await later("else");
  log.push("logical:" + [r1, r2, r3, r4].join("|"));
}
async function templateAndSpread() {
  var s = `a${await later(1)}b${note("t2", 2)}c${await later(3)}`;
  var arr = [await later(1), ...[note("spread", 2), 3], await later(4)];
  var obj = { k: await later("v"), [note("ck", "c")]: await later("w") };
  log.push("tmpl:" + s + " arr:" + arr.join() + " obj:" + JSON.stringify(obj));
}
async function updateAndDelete() {
  var o = { a: 1, b: 2 };
  var key = "a";
  o[await later(key)]++;
  delete o[await later("b")];
  log.push("upd:" + JSON.stringify(o));
}
async function awaitInArguments() {
  var r = Math.max(await later(1), await later(7), await later(3));
  var s = [await later("x"), await later("y")].join("-");
  log.push("args:" + r + " " + s);
}
async function awaitInCondition() {
  var n = 0;
  while (await later(n < 3)) { n++; }
  do { n++; } while (await later(n < 6));
  for (var i = 0; await later(i < 2); i += await later(1)) { n += 10; }
  if (await later(true)) { n += 100; }
  switch (await later(2)) { case await later(1): n += 1000; break; case await later(2): n += 2000; break; }
  log.push("cond:" + n);
}
async function awaitStatementForms() {
  await later(1);
  await 2;
  var x = await await later(3);
  var y = (await later(4)) + (await later(5));
  log.push("stmt:" + x + "," + y);
}
async function awaitInNew() {
  function Box(v) { this.v = v; }
  var b = new Box(await later("boxed"));
  var c = new (await later(Box))("cls");
  log.push("new:" + b.v + "," + c.v);
}
async function sequenceAndComma() {
  var r = (note("s1"), await later("s2"), note("s3"), await later("s4"));
  log.push("seq:" + r);
}
var all = [operandsBeforeAwait, calleeAndThis, binaryLeftFirst, memberAssign, compoundAssign, logical, templateAndSpread, updateAndDelete, awaitInArguments, awaitInCondition, awaitStatementForms, awaitInNew, sequenceAndComma];
(async function () {
  for (var i = 0; i < all.length; i++) {
    log.push("--" + all[i].name);
    await all[i]();
  }
})().then(function () {
  assert.compareArray(log, EXPECTED);
}).then($DONE, $DONE);
