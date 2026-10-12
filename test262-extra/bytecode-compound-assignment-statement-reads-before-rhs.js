/*---
description: >
  A compound assignment in statement context reads the current value (GetValue
  of the reference) before evaluating the right-hand side, and writes after
  it, so getter, right-hand side and setter run in that order and the old
  value is the one combined.
info: |
  sec-assignment-operators-runtime-semantics-evaluation: lval = GetValue(lref)
  precedes the evaluation of AssignmentExpression; PutValue(lref, r) follows.
esid: sec-assignment-operators-runtime-semantics-evaluation
flags: [noStrict]
---*/

var log = [];

globalThis.c1 = 10;
globalThis.c2 = 10;
globalThis.c3 = 6;

function rewriteC1() { globalThis.c1 = 100; return 1; }
function rewriteC2() { globalThis.c2 = 100; return 1; }

function addStatement() { c1 += rewriteC1(); }
function subStatement() { c2 -= rewriteC2(); }

addStatement();
assert.sameValue(c1, 11, "+= combines the value read before the RHS rewrote it");
subStatement();
assert.sameValue(c2, 9, "-= combines the value read before the RHS rewrote it");

var stored = 4;
Object.defineProperty(globalThis, "acc", {
  configurable: true,
  get: function () { log.push("get"); return stored; },
  set: function (v) { log.push("set:" + v); stored = v; },
});

function rhs() { log.push("rhs"); return 5; }
function accStatement() { acc += rhs(); }
function accForUpdate() { for (var i = 0; i < 1; i++, acc *= rhs()) {} }

accStatement();
assert.sameValue(log.join(","), "get,rhs,set:9");
assert.sameValue(stored, 9);

log.length = 0;
accForUpdate();
assert.sameValue(log.join(","), "get,rhs,set:45");

function shiftStatement() { c3 <<= 2; c3 |= 1; c3 ^= 3; }
shiftStatement();
assert.sameValue(c3, 26, "bitwise compound assignments in statement context");
