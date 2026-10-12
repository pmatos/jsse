/*---
description: >
  Update expressions on warmed global vars keep ToNumeric semantics: number,
  string, BigInt, undefined, valueOf objects, non-writable and accessor
  properties, with prefix/postfix result values.
info: |
  Postfix and Prefix Increment/Decrement (sec-postfix-increment-operator,
  sec-prefix-increment-operator) apply ToNumeric to GetValue(lhs), then PutValue.
esid: sec-postfix-increment-operator
flags: [noStrict]
---*/

var n = 1;
for (var i = 0; i < 10; i++) { n++; }
assert.sameValue(n, 11);
assert.sameValue(n++, 11);
assert.sameValue(++n, 13);
assert.sameValue(n--, 13);
assert.sameValue(--n, 11);

var s = "5";
assert.sameValue(s++, 5);
assert.sameValue(s, 6);
var b = 10n;
assert.sameValue(b++, 10n);
assert.sameValue(++b, 12n);
var u;
assert.sameValue(u++, NaN);
assert.sameValue(u, NaN);

var calls = [];
var obj = { valueOf: function () { calls.push("valueOf"); return 4; } };
var ov = obj;
assert.sameValue(ov++, 4);
assert.sameValue(ov, 5);
assert.sameValue(calls.join(), "valueOf");

globalThis.nwv = 3;
for (var j = 0; j < 5; j++) { nwv++; }
Object.defineProperty(globalThis, "nwv", { writable: false });
assert.sameValue(nwv++, 8, "non-writable: result value still computed");
assert.sameValue(nwv, 8);
assert.throws(TypeError, function () { "use strict"; nwv++; });

var trace = [];
var store = 1;
Object.defineProperty(globalThis, "acc", {
  configurable: true,
  get: function () { trace.push("get"); return store; },
  set: function (v) { trace.push("set" + v); store = v; },
});
assert.sameValue(acc++, 1);
assert.sameValue(++acc, 3);
assert.sameValue(trace.join(), "get,set2,get,set3");
