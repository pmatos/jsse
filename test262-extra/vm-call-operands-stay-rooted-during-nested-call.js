// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-evaluatecall
description: >
  EvaluateCall: the callee, thisValue and every evaluated argument stay live
  for the whole [[Call]]/[[Construct]], even when a collection runs inside the
  callee and the only reference is the caller's pending operand list. Covers a
  callee that throws, one that re-enters through a built-in callback, and
  `new` operands.
features: [Reflect.construct]
---*/

function gcNow() {
  if (typeof $262 !== "undefined" && $262.gc) $262.gc();
}

function makeObject(tag) {
  return { tag: tag, payload: [tag, tag + 1, tag + 2] };
}

function consume(a, b, c) {
  gcNow();
  churn();
  return a.tag + b.tag + c.tag + a.payload[2] + b.payload[2] + c.payload[2];
}

function churn() {
  var junk = [];
  for (var i = 0; i < 200; i++) junk.push({ i: i, s: "k" + i });
  return junk.length;
}

function caller() {
  return consume(makeObject(1), makeObject(10), makeObject(100));
}
assert.sameValue(caller(), (1 + 10 + 100) + (3 + 12 + 102), "plain call operands");

function thrower(a, b) {
  gcNow();
  churn();
  throw { a: a.tag, b: b.tag };
}
function throwingCaller() {
  try {
    thrower(makeObject(7), makeObject(8));
  } catch (e) {
    return e.a * 100 + e.b;
  }
}
assert.sameValue(throwingCaller(), 708, "operands survive a throwing callee");

function reentrant(list) {
  return list.map(function (obj, index) {
    gcNow();
    return consume(obj, makeObject(index), makeObject(index + 1));
  });
}
var reentrantResult = reentrant([makeObject(2), makeObject(3)]);
assert.sameValue(reentrantResult.length, 2, "re-entrant length");
assert.sameValue(reentrantResult[0], 2 + 0 + 1 + 4 + 2 + 3, "re-entrant first");
assert.sameValue(reentrantResult[1], 3 + 1 + 2 + 5 + 3 + 4, "re-entrant second");

function Box(a, b) {
  gcNow();
  churn();
  this.sum = a.tag + b.tag;
}
function constructCaller() {
  return new Box(makeObject(4), makeObject(5)).sum;
}
assert.sameValue(constructCaller(), 9, "construct operands");

function boundCaller() {
  var bound = consume.bind(null, makeObject(1));
  return bound(makeObject(2), makeObject(3));
}
assert.sameValue(boundCaller(), 1 + 2 + 3 + 3 + 4 + 5, "bound function operands");

function deepInner(depth, obj) {
  if (depth === 0) {
    gcNow();
    return obj.tag + obj.payload[1];
  }
  return deepInner(depth - 1, makeObject(depth)) + obj.tag;
}
assert.sameValue(deepInner(5, makeObject(1)), 18, "nested recursion");
