// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
description: >
  Array index properties created after the Array are included by
  [[OwnPropertyKeys]]. SetIntegrityLevel must therefore update their property
  descriptors when Object.freeze or Object.seal is applied.
info: |
  10.1.11 OrdinaryOwnPropertyKeys ( O )
    2. For each own property key P of O such that P is an array index, in
       ascending numeric index order, append P to keys.

  7.3.16 SetIntegrityLevel ( O, level )
    3. Let keys be ? O.[[OwnPropertyKeys]]().
    4. If level is sealed, set every own property's [[Configurable]] to false.
    5. If level is frozen, also set every own data property's [[Writable]] to
       false.
features: [Reflect, Symbol]
includes: [propertyHelper.js]
---*/

function assertKeys(actual, expected, message) {
  assert.sameValue(actual.length, expected.length, message + ": key count");
  for (var i = 0; i < expected.length; i++) {
    assert.sameValue(actual[i], expected[i], message + ": key " + i);
  }
}

var pushed = [];
pushed.push(1, 2);
assertKeys(Object.getOwnPropertyNames(pushed), ["0", "1", "length"], "push names");
assertKeys(Reflect.ownKeys(pushed), ["0", "1", "length"], "push ownKeys");

var assigned = new Array(2);
assigned[0] = 1;
assigned[1] = 2;
assertKeys(Object.getOwnPropertyNames(assigned), ["0", "1", "length"], "assigned names");
assertKeys(Reflect.ownKeys(assigned), ["0", "1", "length"], "assigned ownKeys");

var filled = new Array(2).fill(0);
assertKeys(Object.getOwnPropertyNames(filled), ["0", "1", "length"], "fill names");
assertKeys(Reflect.ownKeys(filled), ["0", "1", "length"], "fill ownKeys");

var ordered = [];
var marker = Symbol("marker");
ordered.named = true;
ordered[2] = "c";
ordered[0] = "a";
ordered[marker] = true;
assertKeys(
  Object.getOwnPropertyNames(ordered),
  ["0", "2", "length", "named"],
  "dynamic index ordering and holes"
);
assertKeys(
  Reflect.ownKeys(ordered),
  ["0", "2", "length", "named", marker],
  "dynamic index, string, and symbol ordering"
);
assert.sameValue(Object.hasOwn(ordered, "1"), false, "a hole is not an own property");

var frozen = [];
frozen.push(1, 2);
assert.sameValue(Object.freeze(frozen), frozen, "freeze returns the array");
verifyProperty(frozen, "0", {
  value: 1,
  writable: false,
  enumerable: true,
  configurable: false,
});
verifyProperty(frozen, "1", {
  value: 2,
  writable: false,
  enumerable: true,
  configurable: false,
});
assert.sameValue(Reflect.set(frozen, "0", 99), false, "frozen element rejects writes");
assert.sameValue(Reflect.deleteProperty(frozen, "0"), false, "frozen element rejects deletion");
assert.sameValue(frozen[0], 1, "frozen element value is preserved");
assert.sameValue(Object.isFrozen(frozen), true, "dynamic array is frozen");

var sealed = [];
sealed.push(1);
assert.sameValue(Object.seal(sealed), sealed, "seal returns the array");
verifyProperty(sealed, "0", {
  value: 1,
  writable: true,
  enumerable: true,
  configurable: false,
});
assert.sameValue(Reflect.deleteProperty(sealed, "0"), false, "sealed element rejects deletion");
assert.sameValue(Reflect.set(sealed, "0", 2), true, "sealed element remains writable");
assert.sameValue(sealed[0], 2, "sealed element accepts writes");
assert.sameValue(Object.isSealed(sealed), true, "dynamic array is sealed");
assert.sameValue(Object.isFrozen(sealed), false, "writable sealed array is not frozen");

var sparse = new Array(2);
sparse[1] = 1;
Object.freeze(sparse);
assert.sameValue(Object.hasOwn(sparse, "0"), false, "freeze does not materialize a hole");
verifyProperty(sparse, "1", {
  value: 1,
  writable: false,
  enumerable: true,
  configurable: false,
});
