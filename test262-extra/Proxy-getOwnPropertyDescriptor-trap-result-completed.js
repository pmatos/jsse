// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-proxy-object-internal-methods-and-internal-slots-getownproperty-p
description: >
  The result of a Proxy getOwnPropertyDescriptor trap is run through
  CompletePropertyDescriptor before it is validated and returned, so absent
  writable/enumerable/configurable fields default to false and absent
  value/get/set default to undefined.
info: |
  [[GetOwnProperty]] ( P )
    ...
    12. Let targetDesc be ? target.[[GetOwnProperty]](P).
    14. Let extensibleTarget be ? IsExtensible(target).
    15. Let resultDesc be ? ToPropertyDescriptor(trapResultObj).
    16. Call CompletePropertyDescriptor(resultDesc).
    17. Let valid be IsCompatiblePropertyDescriptor(extensibleTarget, resultDesc, targetDesc).
    18. If valid is false, throw a TypeError exception.
    19. If resultDesc.[[Configurable]] is false, then
      a. If targetDesc is undefined or targetDesc.[[Configurable]] is true, throw a TypeError exception.
      b. If resultDesc has a [[Writable]] field and resultDesc.[[Writable]] is false, then
        i. If targetDesc.[[Writable]] is true, throw a TypeError exception.
    20. Return resultDesc.
includes: [compareArray.js, propertyHelper.js]
---*/

function proxyReturning(target, result) {
  return new Proxy(target, {
    getOwnPropertyDescriptor() { return result; }
  });
}

// Missing fields are completed, observable through Object.getOwnPropertyDescriptor.
var d = Object.getOwnPropertyDescriptor(proxyReturning({ a: 1 }, { value: 1, configurable: true }), "a");
assert.sameValue(d.value, 1, "value");
assert.sameValue(d.writable, false, "writable defaults to false");
assert.sameValue(d.enumerable, false, "enumerable defaults to false");
assert.sameValue(d.configurable, true, "configurable kept");

d = Object.getOwnPropertyDescriptor(proxyReturning({ a: 1 }, { configurable: true }), "a");
assert.sameValue(Object.prototype.hasOwnProperty.call(d, "value"), true, "generic: value present");
assert.sameValue(d.value, undefined, "generic: value defaults to undefined");
assert.sameValue(d.writable, false, "generic: writable defaults to false");
assert.sameValue(d.enumerable, false, "generic: enumerable defaults to false");

d = Object.getOwnPropertyDescriptor(proxyReturning({ a: 1 }, { get: undefined, configurable: true }), "a");
assert.sameValue(Object.prototype.hasOwnProperty.call(d, "set"), true, "accessor: set present");
assert.sameValue(d.set, undefined, "accessor: set defaults to undefined");
assert.sameValue(Object.prototype.hasOwnProperty.call(d, "writable"), false, "accessor: no writable");
assert.sameValue(d.enumerable, false, "accessor: enumerable defaults to false");

d = Object.getOwnPropertyDescriptor(proxyReturning({ a: 1 }, { set: undefined, configurable: true }), "a");
assert.sameValue(Object.prototype.hasOwnProperty.call(d, "get"), true, "accessor: get present");
assert.sameValue(d.get, undefined, "accessor: get defaults to undefined");

// A completed descriptor is checked against the target: the missing
// "configurable" becomes false, which a configurable target property rejects.
assert.throws(TypeError, function() {
  Object.getOwnPropertyDescriptor(proxyReturning({ a: 1 }, { value: 1 }), "a");
}, "completed configurable:false for a configurable target property");

// A missing "writable" becomes false, which a writable non-configurable target property rejects.
var target = {};
Object.defineProperty(target, "w", { value: 1, writable: true, enumerable: true, configurable: false });
assert.throws(TypeError, function() {
  Object.getOwnPropertyDescriptor(proxyReturning(target, { value: 1, enumerable: true, configurable: false }), "w");
}, "completed writable:false for a writable non-configurable target property");

// A missing "enumerable" becomes false, which differs from an enumerable non-configurable target property.
assert.throws(TypeError, function() {
  Object.getOwnPropertyDescriptor(proxyReturning(target, { value: 1, writable: true, configurable: false }), "w");
}, "completed enumerable:false for an enumerable non-configurable target property");

// A fully matching, non-configurable, non-enumerable result is accepted.
var target2 = {};
Object.defineProperty(target2, "n", { value: 1, writable: false, enumerable: false, configurable: false });
d = Object.getOwnPropertyDescriptor(proxyReturning(target2, { value: 1 }), "n");
assert.sameValue(d.value, 1, "matching: value");
assert.sameValue(d.writable, false, "matching: writable");
assert.sameValue(d.enumerable, false, "matching: enumerable");
assert.sameValue(d.configurable, false, "matching: configurable");

// A completed non-configurable result for a property absent from the target is rejected.
assert.throws(TypeError, function() {
  Object.getOwnPropertyDescriptor(proxyReturning({}, { value: 1 }), "missing");
}, "non-configurable result for absent target property");

// Non-extensible target, absent property.
var frozenless = Object.preventExtensions({});
assert.throws(TypeError, function() {
  Object.getOwnPropertyDescriptor(proxyReturning(frozenless, { value: 1, configurable: true }), "x");
}, "descriptor for absent property of non-extensible target");

// The issue's reproducer: Reflect.set with the proxy as receiver.
var calls = [];
var receiver = new Proxy({ a: 1 }, {
  getOwnPropertyDescriptor() { return { value: 1, configurable: true }; },
  defineProperty(t, k, desc) { calls.push(k); return Reflect.defineProperty(t, k, desc); }
});
assert.sameValue(Reflect.set({}, "a", 2, receiver), false,
  "completed descriptor is non-writable, so OrdinarySetWithOwnDescriptor returns false");
assert.sameValue(calls.length, 0, "defineProperty trap not reached");

// Object.prototype.propertyIsEnumerable / Object.keys / Object.entries see enumerable:false.
var p = proxyReturning({ a: 1 }, { value: 1, configurable: true });
assert.sameValue(Object.prototype.propertyIsEnumerable.call(p, "a"), false, "propertyIsEnumerable");

var ownKeysProxy = new Proxy({ a: 1, b: 2 }, {
  getOwnPropertyDescriptor(t, k) {
    return k === "a" ? { value: 1, configurable: true } : { value: 2, enumerable: true, configurable: true };
  }
});
assert.compareArray(Object.keys(ownKeysProxy), ["b"], "Object.keys skips the completed non-enumerable a");

// Nested Proxy target: the target's [[GetOwnProperty]] and [[IsExtensible]] go through its own traps.
var log = [];
var innerTarget = {};
Object.defineProperty(innerTarget, "k", { value: 1, writable: true, enumerable: true, configurable: false });
var inner = new Proxy(innerTarget, {
  getOwnPropertyDescriptor(t, k) {
    log.push("inner-gopd");
    return { value: 1, writable: true, enumerable: true, configurable: false };
  },
  isExtensible(t) { log.push("inner-isExtensible"); return true; }
});
var outer = new Proxy(inner, {
  getOwnPropertyDescriptor() { return { value: 1, enumerable: true, configurable: false }; }
});
assert.throws(TypeError, function() {
  Object.getOwnPropertyDescriptor(outer, "k");
}, "completed writable:false against nested proxy target's writable non-configurable property");
assert.compareArray(log, ["inner-gopd", "inner-isExtensible"], "nested target traps, in spec order");
