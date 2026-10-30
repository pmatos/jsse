// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-createregexpstringiterator
description: >
  RegExp String Iterator objects keep their state in internal slots, so they
  have no own properties observable through reflection, and the unicode
  handling does not depend on any property lookup.
info: |
  22.2.9.1 CreateRegExpStringIterator ( R, S, global, fullUnicode )
    2. Let iterator be OrdinaryObjectCreate(%RegExpStringIteratorPrototype%,
       [[IteratingRegExp]], [[IteratedString]], [[Global]], [[Unicode]],
       [[Done]]).
includes: [compareArray.js]
features: [Reflect, String.prototype.matchAll]
---*/

var it = "x".matchAll(/x/g);
assert.compareArray(Object.getOwnPropertyNames(it), [], "getOwnPropertyNames");
assert.compareArray(Object.keys(it), [], "Object.keys");
assert.compareArray(Reflect.ownKeys(it), [], "Reflect.ownKeys");
assert.sameValue(Object.getOwnPropertyDescriptor(it, "__full_unicode__"), undefined, "descriptor");

var forInKeys = [];
for (var k in it) forInKeys.push(k);
assert.compareArray(forInKeys, [], "for-in");

var uit = /(?:)/gu[Symbol.matchAll]("\u{1F600}");
assert.sameValue(uit.next().value.index, 0, "first empty match");
assert.sameValue(uit.next().value.index, 2, "unicode advance skips the surrogate pair");
assert.sameValue(uit.next().done, true, "done");
assert.compareArray(Reflect.ownKeys(uit), [], "ownKeys after iteration");

var vit = new RegExp("", "gv")[Symbol.matchAll]("\u{1F600}");
assert.sameValue(vit.next().value.index, 0, "v flag: first");
assert.sameValue(vit.next().value.index, 2, "v flag advances by code point");

var own = /(?:)/gu[Symbol.matchAll]("\u{1F600}");
Object.defineProperty(own, "__full_unicode__", { value: false, configurable: true });
own.next();
assert.sameValue(own.next().value.index, 2, "own __full_unicode__ is ignored");

Object.defineProperty(Object.prototype, "__full_unicode__", { value: true, configurable: true });
try {
  var nit = /(?:)/g[Symbol.matchAll]("\u{1F600}");
  assert.sameValue(nit.next().value.index, 0, "first");
  assert.sameValue(nit.next().value.index, 1, "non-unicode advance ignores inherited __full_unicode__");
} finally {
  delete Object.prototype.__full_unicode__;
}

assert.throws(TypeError, function() {
  Object.getPrototypeOf(it).next.call(Object.create(it));
}, "next on an object merely inheriting from an iterator");
