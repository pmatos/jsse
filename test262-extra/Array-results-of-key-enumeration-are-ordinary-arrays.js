// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-createarrayfromlist
description: >
  Arrays produced by CreateArrayFromList (Object.keys/values/entries/
  getOwnPropertyNames, Reflect.ownKeys, String.prototype.split, ...) have
  ordinary Array semantics: default data properties for every index
  (including present undefined values), length, freezing, redefinition,
  deletion, truncation and own-key enumeration.
info: |
  7.3.18 CreateArrayFromList ( elements )
    5. For each element e of elements, do
      a. Perform ! CreateDataPropertyOrThrow(array, ! ToString(𝔽(n)), e).
includes: [compareArray.js, propertyHelper.js]
flags: [noStrict]
---*/

var keys = Object.keys({ a: 1, b: 2, c: 3 });
assert.sameValue(Array.isArray(keys), true);
assert.sameValue(keys.length, 3);
verifyProperty(keys, "0", { value: "a", writable: true, enumerable: true, configurable: true }, { restore: true });
verifyProperty(keys, "length", { value: 3, writable: true, enumerable: false, configurable: false }, { restore: true });
assert.compareArray(Reflect.ownKeys(keys), ["0", "1", "2", "length"]);
assert.compareArray(Object.getOwnPropertyNames(keys), ["0", "1", "2", "length"]);
assert.compareArray(Object.keys(keys), ["0", "1", "2"]);

// Present undefined values are own properties, not holes.
var values = Object.values({ a: undefined, b: 1, c: undefined });
assert.sameValue(values.length, 3);
assert.sameValue(0 in values, true);
assert.sameValue(2 in values, true);
assert.sameValue(values.hasOwnProperty(0), true);
verifyProperty(values, "0", { value: undefined, writable: true, enumerable: true, configurable: true }, { restore: true });
assert.compareArray(Object.keys(values), ["0", "1", "2"]);
assert.sameValue(JSON.stringify(values), "[null,1,null]");
var seen = [];
values.forEach(function (v, i) { seen.push(i); });
assert.compareArray(seen, [0, 1, 2]);

// Entries are pairs of ordinary arrays.
var entries = Object.entries({ x: 1, y: undefined });
assert.sameValue(entries.length, 2);
assert.compareArray(entries[0], ["x", 1]);
assert.sameValue(entries[1].length, 2);
assert.sameValue(1 in entries[1], true);
assert.compareArray(Object.keys(entries[1]), ["0", "1"]);

// Mutation: redefine, delete, shrink, grow, freeze.
var arr = Object.keys({ p: 1, q: 2, r: 3, s: 4 });
Object.defineProperty(arr, "1", { value: "Q", writable: false });
verifyProperty(arr, "1", { value: "Q", writable: false, enumerable: true, configurable: true }, { restore: true });
arr[1] = "ignored";
assert.sameValue(arr[1], "Q");
delete arr[0];
assert.sameValue(0 in arr, false);
assert.compareArray(Object.keys(arr), ["1", "2", "3"]);
assert.sameValue(arr.length, 4);
arr.length = 3;
assert.compareArray(Object.keys(arr), ["1", "2"]);
arr.push("t");
assert.compareArray(Object.keys(arr), ["1", "2", "3"]);
assert.sameValue(arr[3], "t");
Object.freeze(arr);
assert.sameValue(Object.isFrozen(arr), true);
assert.throws(TypeError, function () { "use strict"; arr[2] = 0; });
assert.throws(TypeError, function () { "use strict"; arr.push(1); });
assert.sameValue(arr[2], "r");

// Accessor on an index of a result array.
var acc = Object.keys({ a: 1, b: 2 });
Object.defineProperty(acc, "0", { get: function () { return "got"; }, configurable: true, enumerable: true });
assert.sameValue(acc[0], "got");
assert.compareArray(Object.keys(acc), ["0", "1"]);
assert.compareArray(Object.values(acc), ["got", "b"]);

// Non-index named properties come after the elements.
var named = Object.keys({ m: 1 });
named.tag = "t";
named[3] = "x";
assert.compareArray(Reflect.ownKeys(named), ["0", "3", "length", "tag"]);

// Other builtins that create arrays from lists.
var parts = "a,b,c".split(",");
verifyProperty(parts, "2", { value: "c", writable: true, enumerable: true, configurable: true }, { restore: true });
assert.compareArray(Reflect.ownKeys(parts), ["0", "1", "2", "length"]);
var own = Reflect.ownKeys({ z: 1, [Symbol.iterator]: 2, 5: 0 });
assert.sameValue(own.length, 3);
assert.sameValue(own[0], "5");
assert.sameValue(own[2], Symbol.iterator);

// Species/Array.from over result arrays.
var viaFrom = Array.from(Object.keys({ a: 1, b: 2 }), function (k) { return k + k; });
assert.compareArray(viaFrom, ["aa", "bb"]);
assert.compareArray(Object.keys({ a: 1, b: 2 }).concat(["c"]), ["a", "b", "c"]);
assert.compareArray(Object.keys({ a: 1, b: 2 }).reverse(), ["b", "a"]);
