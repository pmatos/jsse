// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-array.of
description: >
  Array.of called on %Array% builds the same fresh array the generic steps do:
  own data properties for every element (undefined included), no inherited
  index setter runs, subclass and non-constructor receivers still go through
  Construct, and the result does not depend on the Array global binding.
info: |
  23.1.2.3 Array.of ( ...items )
    4. If IsConstructor(C) is true, then
      a. Let A be ? Construct(C, « 𝔽(len) »).
    5. Else, Let A be ? ArrayCreate(len).
    7. Repeat, while k < len,
      c. Perform ? CreateDataPropertyOrThrow(A, Pk, kValue).
    8. Perform ? Set(A, "length", lenNumber, true).
flags: [noStrict]
includes: [propertyHelper.js, compareArray.js]
features: [Reflect]
---*/

var setterCalls = 0;
Object.defineProperty(Array.prototype, "1", {
  set: function() { setterCalls++; },
  get: function() { return "inherited"; },
  configurable: true
});

var r = Array.of(10, undefined, 30);
assert.sameValue(setterCalls, 0, "inherited index setter not invoked");
assert.sameValue(r.length, 3, "length");
assert(Object.prototype.hasOwnProperty.call(r, "1"), "undefined element is an own property");
assert.sameValue(r[1], undefined, "own undefined shadows inherited accessor");
assert.compareArray(Object.getOwnPropertyNames(r), ["0", "1", "2", "length"], "own keys");
verifyProperty(r, "0", { value: 10, writable: true, enumerable: true, configurable: true });
verifyProperty(r, "length", { writable: true, enumerable: false, configurable: false });
assert.sameValue(Object.getPrototypeOf(r), Array.prototype, "prototype");
assert(Array.isArray(r), "exotic array");

delete Array.prototype[1];

var empty = Array.of();
assert.sameValue(empty.length, 0, "empty length");
assert.compareArray(Object.getOwnPropertyNames(empty), ["length"], "empty keys");
empty[4] = 1;
assert.sameValue(empty.length, 5, "result is a live array exotic object");

class Sub extends Array {}
var s = Array.of.call(Sub, 1, 2);
assert.sameValue(Object.getPrototypeOf(s), Sub.prototype, "subclass receiver");
assert.sameValue(s.length, 2, "subclass length");

var seen;
function Plain(n) { seen = n; this.tag = "plain"; }
var p = Array.of.call(Plain, 7, 8);
assert.sameValue(seen, 2, "Construct called with length");
assert.sameValue(p.tag, "plain", "plain constructor result");
assert.sameValue(p[1], 8, "plain element");

var notCtor = Array.of.call(undefined, 5, 6);
assert.sameValue(Array.isArray(notCtor), true, "non-constructor receiver");
assert.sameValue(notCtor.length, 2, "non-constructor length");

var savedOf = Array.of;
var SavedArray = Array;
assert.sameValue(savedOf.call(SavedArray, 1).length, 1, "intrinsic receiver via saved reference");
SavedArray = undefined;
delete Array.prototype.constructor;
delete globalThis.Array;
$262.gc();
var after = savedOf.call(Object, 1);
assert.sameValue(after.length, 1, "other receiver unaffected after the Array global is gone");
assert.sameValue(savedOf(1, 2, 3).length, 3, "undefined receiver after the Array global is gone");
assert.sameValue([1, 2].length, 2, "array literals still work");
