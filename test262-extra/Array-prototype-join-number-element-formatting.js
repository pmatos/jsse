// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-array.prototype.join
description: >
  Array.prototype.join renders Number elements with Number::toString (integral
  fast path included), and still observes accessor elements, inherited
  elements on holes and length-changing side effects.
info: |
  23.1.3.18 Array.prototype.join ( separator )
    7. Repeat, while k < len,
      b. Let element be ? Get(O, ! ToString(𝔽(k))).
      c. If element is neither undefined nor null, then
        i. Let S be ? ToString(element).
---*/

assert.sameValue([0, -0, 1, -1, 10, -10].join(), "0,0,1,-1,10,-10", "small integers and negative zero");
assert.sameValue([123456789012345, -123456789012345].join(" "), "123456789012345 -123456789012345", "15 digit integers");
assert.sameValue([999999999999999, 1e15, 1e21, 1.5, -1.5, NaN, Infinity, -Infinity].join(), "999999999999999,1000000000000000,1e+21,1.5,-1.5,NaN,Infinity,-Infinity", "integers at and beyond the fast-path bound, and non-integers");
assert.sameValue([4294967295, 2147483648, -2147483649].join(), "4294967295,2147483648,-2147483649", "32 bit boundaries");
assert.sameValue([1, , 3].join("-"), "1--3", "hole renders empty");
assert.sameValue([undefined, null, 0].join("|"), "||0", "nullish elements render empty");

var holeWithProto = [1, , 3];
Array.prototype[1] = 7;
try {
  assert.sameValue(holeWithProto.join(), "1,7,3", "hole reads the prototype");
} finally {
  delete Array.prototype[1];
}

var withGetter = [1, 2, 3];
var gets = 0;
Object.defineProperty(withGetter, 1, { get: function () { gets++; return 42; } });
assert.sameValue(withGetter.join(), "1,42,3", "accessor element");
assert.sameValue(gets, 1, "accessor invoked once");

var shrinking = [1, 2, 3, 4];
Object.defineProperty(shrinking, 1, { get: function () { shrinking.length = 2; return 9; } });
assert.sameValue(shrinking.join(), "1,9,,", "elements removed mid-join read as undefined");

assert.sameValue(Array.prototype.join.call({ length: 3, 0: 5, 1: -6, 2: 7 }, ":"), "5:-6:7", "array-like receiver");

assert.sameValue(Array.prototype.join.call(new Int8Array([1, -2, 3]), "-"), "1--2-3", "typed array receiver");
assert.sameValue(Array.prototype.join.call(new Float64Array([-0, 0.5, 7]), ","), "0,0.5,7", "typed array receiver with non-integral and negative zero");
assert.sameValue(
  Array.prototype.join.call(new Proxy([1, 2, 3], { get: function (t, k) { return k === "length" ? 3 : Number(k) * 10; } }), ","),
  "0,10,20",
  "proxy receiver reads through the get trap"
);
assert.sameValue([1n, -2n].join(), "1,-2", "BigInt elements");
assert.throws(TypeError, function () { [1, Symbol()].join(); }, "Symbol element throws");
assert.sameValue(`${0}${-0}${-5}${999999999999999}${1e15}`, "00-59999999999999991000000000000000", "template literal uses the same Number::toString");
