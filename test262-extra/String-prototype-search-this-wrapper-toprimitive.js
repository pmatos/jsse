// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-string.prototype.indexof
description: >
  The String.prototype search methods apply ToString to a String wrapper this
  value through ToPrimitive, so an own toString, a subclass toString, or an own
  @@toPrimitive on the wrapper is observed instead of reading [[StringData]]
  directly. Expected values are cross-checked with Node.
info: |
  22.1.3.9 String.prototype.indexOf ( searchString [ , position ] )
    1. Let O be ? RequireObjectCoercible(this value).
    2. Let S be ? ToString(O).

  7.1.17 ToString ( argument )
    ... 10. Let primValue be ? ToPrimitive(argument, string).
features: [Symbol.toPrimitive, class]
---*/

var ownToString = new String("abc");
ownToString.toString = function () { return "xyz"; };
assert.sameValue(ownToString.indexOf("x"), 0, "indexOf: own toString");
assert.sameValue(ownToString.lastIndexOf("z"), 2, "lastIndexOf: own toString");
assert.sameValue(ownToString.includes("y"), true, "includes: own toString");
assert.sameValue(ownToString.startsWith("xy"), true, "startsWith: own toString");
assert.sameValue(ownToString.endsWith("yz"), true, "endsWith: own toString");
assert.sameValue(ownToString.includes("a"), false, "includes: [[StringData]] is not used");

class Sub extends String {
  toString() { return "zz"; }
}
assert.sameValue(new Sub("abc").indexOf("zz"), 0, "indexOf: subclass toString");

var ownToPrimitive = new String("abc");
ownToPrimitive[Symbol.toPrimitive] = function (hint) {
  assert.sameValue(hint, "string", "ToPrimitive hint");
  return "\ud800q";
};
assert.sameValue(ownToPrimitive.includes("q"), true, "includes: own @@toPrimitive");
assert.sameValue(ownToPrimitive.indexOf("\ud800"), 0, "indexOf: lone surrogate from @@toPrimitive");
assert.sameValue(ownToPrimitive.indexOf("\udc00"), -1, "indexOf: different lone surrogate from @@toPrimitive");
