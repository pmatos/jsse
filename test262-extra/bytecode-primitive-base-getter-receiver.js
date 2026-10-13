/*---
description: >
  A property read on a primitive base passes the primitive itself, not a
  transient wrapper object, as the receiver of an inherited accessor, so a
  strict getter observes an unboxed `this` (dotted and computed forms, at
  Script top level and inside functions).
info: |
  GetValue (sec-getvalue): if V.[[Base]] is a primitive, baseObj is
  ToObject(V.[[Base]]) and the result is baseObj.[[Get]](V.[[ReferencedName]],
  GetThisValue(V)); GetThisValue of a Property Reference is its [[Base]],
  the primitive (sec-getthisvalue).
esid: sec-getvalue
---*/

Object.defineProperty(Object.prototype, "primitiveBaseGetterProbe", {
  get: function () {
    "use strict";
    return this;
  },
  configurable: true
});

var key = "primitiveBaseGetterProbe";
var sym = Symbol("s");
var big = 10n;

assert.sameValue((5).primitiveBaseGetterProbe, 5, "number, dotted");
assert.sameValue((5)[key], 5, "number, computed");
assert.sameValue("str".primitiveBaseGetterProbe, "str", "string, dotted");
assert.sameValue("str"[key], "str", "string, computed");
assert.sameValue(true.primitiveBaseGetterProbe, true, "boolean");
assert.sameValue(sym.primitiveBaseGetterProbe, sym, "symbol");
assert.sameValue(big.primitiveBaseGetterProbe, big, "bigint");
assert.sameValue("abc".length, 3, "string length");
assert.sameValue("abc"[1], "b", "string index");

function inFunction(n, s) {
  var seen = 0;
  for (var i = 0; i < 3; i++) {
    seen += n.primitiveBaseGetterProbe;
    if (s[key] !== s) throw new Test262Error("string receiver was wrapped");
  }
  return seen;
}
assert.sameValue(inFunction(2, "x"), 6, "inside a function");

delete Object.prototype.primitiveBaseGetterProbe;
