// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-stringpaddingbuiltinsimpl
description: >
  String.prototype.padStart and padEnd return the string unchanged before
  converting fillString when maxLength is not greater than the string length,
  so a throwing or Symbol fillString is never observed.
info: |
  22.1.3.17.1 StringPaddingBuiltinsImpl ( thisValue, maxLength, fillString, placement )
    1. Let string be ? ToString(thisValue).
    2. Let intMaxLength be ℝ(? ToLength(maxLength)).
    3. Let stringLength be the length of string.
    4. If intMaxLength ≤ stringLength, return string.
    5. If fillString is undefined, set fillString to the String value consisting solely of the code unit 0x0020 (SPACE).
    6. Else, set fillString to ? ToString(fillString).
features: [Symbol]
---*/

var calls = 0;
var fill = { toString: function () { calls++; throw new Test262Error("fillString converted"); } };

assert.sameValue("abc".padStart(3, fill), "abc", "padStart at equal length");
assert.sameValue("abc".padEnd(1, fill), "abc", "padEnd below length");
assert.sameValue("abc".padStart(-1, fill), "abc", "padStart negative length");
assert.sameValue(calls, 0, "fillString.toString not called");

assert.sameValue("abc".padStart(2, Symbol()), "abc", "padStart ignores Symbol fill");
assert.sameValue("abc".padEnd(3, Symbol()), "abc", "padEnd ignores Symbol fill");

assert.throws(TypeError, function () { "abc".padStart(4, Symbol()); }, "Symbol fill converted when padding");
