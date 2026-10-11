// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-createunmappedargumentsobject
description: >
  The @@iterator property of mapped and unmapped arguments objects is the
  intrinsic %Array.prototype.values%, captured at realm creation: it does not
  follow later replacement or deletion of Array.prototype[@@iterator], and the
  intrinsic survives a collection after both of its public references are gone.
info: |
  10.4.4.6 CreateUnmappedArgumentsObject
    Perform ! DefinePropertyOrThrow(obj, @@iterator, PropertyDescriptor {
    [[Value]]: %Array.prototype.values%, [[Writable]]: true,
    [[Enumerable]]: false, [[Configurable]]: true }).
  10.4.4.7 CreateMappedArgumentsObject (same step)
flags: [noStrict]
features: [Symbol.iterator]
---*/

var originalValues = Array.prototype.values;
var originalIterator = Array.prototype[Symbol.iterator];
assert.sameValue(originalValues, originalIterator, "precondition");

function mapped() { return arguments; }
function unmapped(a = 0) { return arguments; }
function strict() { "use strict"; return arguments; }

Array.prototype[Symbol.iterator] = function replaced() { throw new Test262Error("replaced iterator used"); };

var m = mapped(1, 2, 3);
var u = unmapped(4, 5);
var s = strict(6);
assert.sameValue(m[Symbol.iterator], originalValues, "mapped after replace");
assert.sameValue(u[Symbol.iterator], originalValues, "unmapped after replace");
assert.sameValue(s[Symbol.iterator], originalValues, "strict after replace");
assert.sameValue([...m].join(), "1,2,3", "mapped spread after replace");
assert.sameValue([...u].join(), "4,5", "unmapped spread after replace");

delete Array.prototype[Symbol.iterator];
var m2 = mapped("a", "b");
assert.sameValue(m2[Symbol.iterator], originalValues, "mapped after delete");
assert.sameValue([...m2].join(), "a,b", "spread after delete");
assert.sameValue(
  Object.getOwnPropertyDescriptor(m2, Symbol.iterator).enumerable,
  false,
  "iterator stays non-enumerable"
);

delete Array.prototype.values;
originalValues = undefined;
originalIterator = undefined;
$262.gc();

var m3 = mapped("x", "y", "z");
assert.sameValue(typeof m3[Symbol.iterator], "function", "intrinsic survives collection");
assert.sameValue([...m3].join(), "x,y,z", "spread after collection");
var it = m3[Symbol.iterator]();
assert.sameValue(it.next().value, "x", "iterator is usable");
