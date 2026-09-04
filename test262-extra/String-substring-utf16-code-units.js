// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-string.prototype.substring
description: >
  String.prototype.substring extracts UTF-16 code units without replacing a
  lone surrogate produced by slicing a surrogate pair.
info: |
  String.prototype.substring ( start, end )
    4. Let len be the length of S.
    ...
    9. Let from be min(finalStart, finalEnd).
    10. Let to be max(finalStart, finalEnd).
    11. Return the substring of S from from to to.

  ECMAScript String values are sequences of 16-bit unsigned integer values, so
  the returned substring can contain an unpaired surrogate code unit.
---*/

var pair = "\ud834\udf06";

var lead = pair.substring(0, 1);
assert.sameValue(lead.length, 1, "lead surrogate result length");
assert.sameValue(lead.charCodeAt(0), 0xd834, "lead surrogate code unit");

var trail = pair.substring(1, 2);
assert.sameValue(trail.length, 1, "trail surrogate result length");
assert.sameValue(trail.charCodeAt(0), 0xdf06, "trail surrogate code unit");

var swapped = pair.substring(2, 1);
assert.sameValue(swapped.length, 1, "swapped indices result length");
assert.sameValue(swapped.charCodeAt(0), 0xdf06, "swapped indices code unit");

var wrapped = new String(pair).substring(0, 1);
assert.sameValue(wrapped.length, 1, "String wrapper result length");
assert.sameValue(wrapped.charCodeAt(0), 0xd834, "String wrapper code unit");
