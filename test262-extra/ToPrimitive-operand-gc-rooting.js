// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-toprimitive
description: >
  The object being converted by ToPrimitive stays reachable while its
  @@toPrimitive, toString and valueOf lookups and calls run user code that
  triggers a garbage collection, even though a fresh operand (the result of
  Object(1n)) has no other reference.
info: |
  ToPrimitive ( input [ , preferredType ] )

  2. If input is an Object, then
    a. Let exoticToPrim be ? GetMethod(input, @@toPrimitive).
    ...
    d. Return ? OrdinaryToPrimitive(input, preferredType).

  OrdinaryToPrimitive ( O, hint )

  3. For each element name of methodNames, do
    a. Let method be ? Get(O, name).
    b. If IsCallable(method) is true, then
      i. Let result be ? Call(method, O).
features: [BigInt, host-gc-required]
---*/

function collect() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

const BigIntToString = BigInt.prototype.toString;
Object.defineProperty(BigInt.prototype, "toString", {
  get: () => {
    collect();
    return function() {
      return `${BigIntToString.call(this)}foo`;
    };
  },
});

assert.sameValue((() => `${Object(1n)}`)(), "1foo", "template literal conversion");
assert.sameValue((() => ({ "1foo": 1 })[Object(1n)])(), 1, "property key conversion");
assert.sameValue((() => "".concat(Object(1n)))(), "1foo", "String.prototype.concat conversion");
assert.sameValue((() => String(Object(1n)))(), "1foo", "String() conversion");

var viaToPrimitive = {
  get [Symbol.toPrimitive]() {
    collect();
    return function() {
      return 7;
    };
  }
};
assert.sameValue(+viaToPrimitive, 7, "@@toPrimitive getter that collects");
