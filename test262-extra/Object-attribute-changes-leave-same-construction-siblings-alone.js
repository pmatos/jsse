// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-validateandapplypropertydescriptor
description: >
  Changing the attributes of one object's properties (freeze, seal, redefining
  as non-enumerable, non-writable or an accessor) does not touch the objects
  built by the same constructor, whatever the order of the changes.
info: |
  10.1.6.3 ValidateAndApplyPropertyDescriptor ( O, P, extensible, Desc, current )
  20.1.2.6 Object.freeze ( O )
  20.1.2.20 Object.seal ( O )
includes: [compareArray.js, propertyHelper.js]
features: [Reflect]
---*/

function Rec(n) {
  this.a = n;
  this.b = n + 1;
  this.c = n + 2;
  this.d = n + 3;
  this.e = n + 4;
  this.f = n + 5;
}

function same(o, n, label) {
  var keys = Object.keys(o);
  assert.compareArray(keys, ["a", "b", "c", "d", "e", "f"], label + " keys");
  for (var i = 0; i < keys.length; i++) {
    verifyProperty(o, keys[i], { value: n + i, writable: true, enumerable: true, configurable: true }, { restore: true });
  }
  assert.sameValue(Object.isExtensible(o), true, label + " extensible");
}

var frozen = new Rec(0), sealed = new Rec(10), hidden = new Rec(20), ro = new Rec(30);
var acc = new Rec(40), plain = new Rec(50), late = new Rec(60);

Object.freeze(frozen);
Object.seal(sealed);
Object.defineProperty(hidden, "c", { enumerable: false });
Object.defineProperty(ro, "b", { writable: false });
Object.defineProperty(acc, "d", { get: function () { return "got"; }, configurable: true });

assert.sameValue(Object.isFrozen(frozen), true, "frozen");
assert.sameValue(Object.isSealed(sealed), true, "sealed");
assert.sameValue(Object.isFrozen(sealed), false, "sealed is not frozen");
assert.compareArray(Object.keys(hidden), ["a", "b", "d", "e", "f"], "c hidden");
assert.compareArray(Reflect.ownKeys(hidden), ["a", "b", "c", "d", "e", "f"], "order unchanged");
assert.sameValue(Reflect.set(ro, "b", 0), false, "read-only b");
assert.sameValue(ro.b, 31, "b unchanged");
assert.sameValue(acc.d, "got", "accessor d");
assert.sameValue(Object.getOwnPropertyDescriptor(acc, "d").set, undefined, "accessor has no setter");

same(plain, 50, "plain sibling");
same(late, 60, "sibling built after the changes");

assert.sameValue(Reflect.set(frozen, "a", 99), false, "frozen a");
assert.sameValue(Reflect.deleteProperty(sealed, "a"), false, "sealed a not deletable");
sealed.a = 11;
assert.sameValue(sealed.a, 11, "sealed a still writable");

// Redefine in the opposite order on fresh siblings.
var x = new Rec(70), y = new Rec(80);
Object.defineProperty(y, "f", { enumerable: false, writable: false });
Object.defineProperty(x, "a", { get: function () { return 1; }, configurable: true });
Object.defineProperty(x, "a", { value: "data", writable: true });
assert.sameValue(x.a, "data", "accessor back to data");
verifyProperty(x, "a", { value: "data", writable: true, enumerable: true, configurable: true }, { restore: true });
assert.compareArray(Object.keys(y), ["a", "b", "c", "d", "e"], "f hidden on y only");
same(new Rec(90), 90, "built after y");
