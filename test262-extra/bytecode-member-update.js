/*---
description: >
  ++/-- on member references evaluate the base and key once, convert with
  ToNumeric, call accessors exactly once each and produce the numeric old
  value for postfix forms, in compiled and tree-walked function bodies alike.
info: |
  jsse issue #902: member updates compile to a dedicated opcode. This file
  also pins the tree-walker, which used to evaluate the base and key
  expressions a second time when the base was a primitive.

  Postfix Increment Operator (13.4.2.1), Prefix Increment Operator
  (13.4.4.1), GetValue (6.2.5.5), PutValue (6.2.5.6), ToNumeric (7.1.3).
esid: sec-postfix-increment-operator-runtime-semantics-evaluation
flags: [noStrict]
---*/

function postfixAndPrefix() {
  var o = { x: 1, y: 5 };
  var k = "y";
  var r = [];
  r.push(o.x++, ++o.x, o.x--, --o.x, o[k]++, o[k], --o[k]);
  return r.join() + "|" + o.x + o.y;
}
assert.sameValue(postfixAndPrefix(), "1,3,3,1,5,6,5|15");

function baseAndKeyEvaluatedOnce() {
  var baseCalls = 0, keyCalls = 0;
  var o = { a: 1 };
  function base() { baseCalls++; return o; }
  function key() { keyCalls++; return "a"; }
  base().a++;
  base()[key()]--;
  ++base()[key()];
  return [baseCalls, keyCalls, o.a].join();
}
assert.sameValue(baseAndKeyEvaluatedOnce(), "3,2,2");

function primitiveBaseEvaluatedOnce() {
  var baseCalls = 0, keyCalls = 0;
  function base() { baseCalls++; return "abc"; }
  function key() { keyCalls++; return "length"; }
  var r1, r2;
  r1 = base().length++;
  r2 = base()[key()]++;
  return [r1, r2, baseCalls, keyCalls].join();
}
assert.sameValue(primitiveBaseEvaluatedOnce(), "3,3,2,1");

function numericConversion() {
  var o = { s: "5", n: null, v: { valueOf: function () { return 7; } }, b: 1n, u: undefined };
  var a = o.s++;
  var b = o.n++;
  var c = o.v++;
  var d = o.b++;
  var e = o.u++;
  return [typeof a, a, b, c, d, e, o.s, o.n, o.v, o.b, o.u].join();
}
assert.sameValue(numericConversion(), "number,5,0,7,1,NaN,6,1,8,2,NaN");

function accessorsRunOncePerUpdate() {
  var gets = 0, sets = 0, v = 10;
  var o = {
    get x() { gets++; return v; },
    set x(n) { sets++; v = n; }
  };
  var old = o.x++;
  return [old, v, gets, sets].join();
}
assert.sameValue(accessorsRunOncePerUpdate(), "10,11,1,1");

function symbolOperandThrows() {
  var o = { s: Symbol("x") };
  try {
    o.s++;
  } catch (e) {
    return e.constructor === TypeError;
  }
  return false;
}
assert.sameValue(symbolOperandThrows(), true);

function nullishBaseThrowsTypeError() {
  var r = [];
  [null, undefined].forEach(function (base) {
    try { base.x++; } catch (e) { r.push(e.constructor === TypeError); }
    try { base["k"]--; } catch (e) { r.push(e.constructor === TypeError); }
  });
  return r.join();
}
assert.sameValue(nullishBaseThrowsTypeError(), "true,true,true,true");

function frozenObjectSloppyIsSilent() {
  var o = Object.freeze({ x: 1 });
  var r = o.x++;
  return r + ":" + o.x;
}
assert.sameValue(frozenObjectSloppyIsSilent(), "1:1");

function frozenObjectStrictThrows() {
  "use strict";
  var o = Object.freeze({ x: 1 });
  try {
    o.x++;
  } catch (e) {
    return e.constructor === TypeError;
  }
  return false;
}
assert.sameValue(frozenObjectStrictThrows(), true);

function keyCoercedOnce() {
  var calls = 0;
  var key = { toString: function () { calls++; return "p"; } };
  var o = { p: 1 };
  o[key]++;
  return calls + ":" + o.p;
}
assert.sameValue(keyCoercedOnce(), "1:2");
