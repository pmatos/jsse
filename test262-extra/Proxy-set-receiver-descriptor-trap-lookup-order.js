// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-ordinarysetwithowndescriptor
description: >
  OrdinarySetWithOwnDescriptor with a Proxy receiver looks up the handler's
  getOwnPropertyDescriptor and defineProperty traps in spec order (once each,
  observably), builds a descriptor object only for traps that exist, hands a
  defineProperty trap exactly the fields of the record, and never consults
  Object.prototype while forwarding to the target.
info: |
  10.1.9.2 OrdinarySetWithOwnDescriptor ( O, P, V, Receiver, ownDesc )
    2.c. Let existingDescriptor be ? Receiver.[[GetOwnProperty]](P).
    2.d. If existingDescriptor is not undefined, then
      i. If IsAccessorDescriptor(existingDescriptor) is true, return false.
      ii. If existingDescriptor.[[Writable]] is false, return false.
      iii. Let valueDesc be the PropertyDescriptor { [[Value]]: V }.
      iv. Return ? Receiver.[[DefineOwnProperty]](P, valueDesc).
    2.e. Else, Return ? CreateDataProperty(Receiver, P, V).
  10.5.5 [[GetOwnProperty]] ( P ), 10.5.6 [[DefineOwnProperty]] ( P, Desc )
    GetMethod(handler, "getOwnPropertyDescriptor") / GetMethod(handler, "defineProperty")
---*/

function loggingHandler(log, handler) {
  return new Proxy(handler, {
    get(t, k, r) {
      if (typeof k === "string") log.push("get:" + k);
      return Reflect.get(t, k, r);
    }
  });
}

// 1. No descriptor traps, existing property: both lookups, in order, no extras.
var log = [];
var target = { x: 1 };
var p = new Proxy(target, loggingHandler(log, {}));
assert.sameValue(Reflect.set(target, "x", 2, p), true, "existing: result");
assert.sameValue(target.x, 2, "existing: written to target");
assert.sameValue(
  log.join(), "get:getOwnPropertyDescriptor,get:defineProperty", "existing: lookup order");

// 2. No descriptor traps, absent property: same lookups, property created.
log.length = 0;
assert.sameValue(Reflect.set(target, "y", 5, p), true, "absent: result");
assert.sameValue(Object.getOwnPropertyDescriptor(target, "y").enumerable, true, "absent: enumerable");
assert.sameValue(Object.getOwnPropertyDescriptor(target, "y").configurable, true, "absent: configurable");
assert.sameValue(Object.getOwnPropertyDescriptor(target, "y").writable, true, "absent: writable");
assert.sameValue(log.join(), "get:getOwnPropertyDescriptor,get:defineProperty", "absent: lookup order");

// 3. p.x = v: the set trap lookup comes first.
log.length = 0;
p.x = 9;
assert.sameValue(target.x, 9, "assignment: written to target");
assert.sameValue(
  log.join(),
  "get:set,get:getOwnPropertyDescriptor,get:defineProperty",
  "assignment: lookup order");

// 4. Object.prototype must not leak into the internal descriptor records: an
//    inherited non-callable "get"/"set" would make ToPropertyDescriptor throw if
//    a descriptor *object* were round-tripped.
Object.prototype.get = 5;
Object.prototype.set = 5;
var t4 = { a: 1 };
try {
  var p4 = new Proxy(t4, Object.create(null));
  assert.sameValue(Reflect.set(t4, "a", 2, p4), true, "polluted: existing");
  assert.sameValue(Reflect.set(t4, "b", 3, p4), true, "polluted: absent");
  p4.a = 4;
  assert.sameValue(Reflect.set({}, "c", 1, p4), true, "polluted: forwarded create");
} finally {
  delete Object.prototype.get;
  delete Object.prototype.set;
}
assert.sameValue(t4.a, 4, "polluted: value");
assert.sameValue(t4.b, 3, "polluted: created value");

// 5. defineProperty trap receives exactly the record's fields.
var defined = [];
var t5 = { a: 1 };
var p5 = new Proxy(t5, {
  defineProperty(t, k, d) {
    defined.push(k + ":" + Reflect.ownKeys(d).join("|"));
    return Reflect.defineProperty(t, k, d);
  }
});
Reflect.set(t5, "a", 2, p5);
Reflect.set(t5, "b", 3, p5);
assert.sameValue(defined.join(), "a:value,b:value|writable|enumerable|configurable", "trap descriptor fields");
assert.sameValue(t5.a, 2, "trap: existing written");
assert.sameValue(t5.b, 3, "trap: created");

// 6. getOwnPropertyDescriptor trap result drives the outcome; no defineProperty
//    call (or lookup) when the record is an accessor or non-writable.
var calls = [];
var t6 = { a: 1 };
var p6 = new Proxy(t6, loggingHandler(calls, {
  getOwnPropertyDescriptor(t, k) { return gopdResult; },
  defineProperty(t, k, d) { calls.push("define"); return Reflect.defineProperty(t, k, d); }
}));
var gopdResult = { get() {}, set() {}, configurable: true };
assert.sameValue(Reflect.set(t6, "a", 2, p6), false, "accessor record");
gopdResult = { value: 1, writable: false, configurable: true };
assert.sameValue(Reflect.set(t6, "a", 2, p6), false, "non-writable record");
assert.sameValue(calls.join(), "get:getOwnPropertyDescriptor,get:getOwnPropertyDescriptor", "no defineProperty lookup");
gopdResult = { value: 1, writable: true, configurable: true };
calls.length = 0;
assert.sameValue(Reflect.set(t6, "a", 7, p6), true, "writable record");
assert.sameValue(
  calls.join(), "get:getOwnPropertyDescriptor,get:defineProperty,define", "writable record order");
assert.sameValue(t6.a, 7, "writable record: written");

// 7. Trap errors and invariants propagate.
var boom = new Proxy({}, { get(t, k) { if (k === "defineProperty") throw new RangeError("boom"); } });
assert.throws(RangeError, function () {
  Reflect.set({ a: 1 }, "a", 2, new Proxy({ a: 1 }, boom));
}, "handler lookup throws");

var frozen = Object.preventExtensions({});
var p7 = new Proxy(frozen, { defineProperty() { return true; } });
assert.throws(TypeError, function () {
  Reflect.set({}, "z", 1, p7);
}, "defineProperty invariant: non-extensible target");

var p8 = new Proxy({}, { defineProperty() { return false; } });
assert.sameValue(Reflect.set({}, "z", 1, p8), false, "defineProperty trap returns false");

var r = Proxy.revocable({}, {});
r.revoke();
assert.throws(TypeError, function () {
  Reflect.set({ a: 1 }, "a", 2, r.proxy);
}, "revoked receiver");

// 8. Receiver proxy reached through the prototype chain walk (no own property).
var log9 = [];
var recv = new Proxy({}, loggingHandler(log9, {}));
var child = Object.create({});
assert.sameValue(Reflect.set(child, "q", 1, recv), true, "proto-walk result");
assert.sameValue(
  log9.join(), "get:getOwnPropertyDescriptor,get:defineProperty", "proto-walk lookup order");
assert.sameValue(Object.getPrototypeOf(recv), Object.prototype, "recv untouched");
assert.sameValue(recv.q, 1, "proto-walk: value on receiver's target");
