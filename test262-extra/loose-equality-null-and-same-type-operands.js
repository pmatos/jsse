/*---
description: >
  `==` and `!=` with a null or undefined operand, and between two operands of
  one type, give the IsLooselyEqual results in expressions and in the
  condition of `if`/`while`/`?:`, including objects, [[IsHTMLDDA]] objects and
  operands that need ToPrimitive (valueOf runs once and in order).
esid: sec-islooselyequal
features: [IsHTMLDDA]
flags: [noStrict]
---*/

var htmldda = $262.IsHTMLDDA;
var values = [undefined, null, 0, -0, NaN, 1, "", "0", "a", true, false, {}, [], function () {}, Symbol.iterator, 0n, 1n, htmldda];
var nullish = function (v) { return v === null || v === undefined; };

function eq(a, b) { return a == b; }
function ne(a, b) { return a != b; }
function eqIf(a, b) { if (a == b) return true; return false; }
function neIf(a, b) { if (a != b) return true; return false; }
function eqTernary(a, b) { return a == b ? 1 : 2; }

// null and undefined are loosely equal to each other and to IsHTMLDDA only.
values.forEach(function (v, i) {
  [null, undefined].forEach(function (n) {
    var expected = nullish(v) || v === htmldda;
    assert.sameValue(eq(v, n), expected, "value " + i + " == nullish");
    assert.sameValue(eq(n, v), expected, "nullish == value " + i);
    assert.sameValue(ne(v, n), !expected, "value " + i + " != nullish");
    assert.sameValue(ne(n, v), !expected, "nullish != value " + i);
    assert.sameValue(eqIf(v, n), expected, "if (value " + i + " == nullish)");
    assert.sameValue(neIf(n, v), !expected, "if (nullish != value " + i + ")");
    assert.sameValue(eqTernary(v, n), expected ? 1 : 2, "ternary " + i);
  });
});

// Same-type pairs.
var obj = {};
assert.sameValue(eq(obj, obj), true, "same object");
assert.sameValue(eq(obj, {}), false, "different objects");
assert.sameValue(ne(obj, {}), true, "different objects !=");
assert.sameValue(eq("ab", "a" + "b"), true, "equal strings");
assert.sameValue(eq("ab", "ac"), false, "different strings");
assert.sameValue(eq(NaN, NaN), false, "NaN");
assert.sameValue(eq(0, -0), true, "zeros");
assert.sameValue(eq(true, true), true, "booleans");
assert.sameValue(eq(true, false), false, "booleans differ");
assert.sameValue(eq(Symbol.iterator, Symbol.iterator), true, "same symbol");
assert.sameValue(eq(Symbol("s"), Symbol("s")), false, "different symbols");
assert.sameValue(eq(10n, 10n), true, "equal BigInts");
assert.sameValue(eq(10n, 11n), false, "different BigInts");
assert.sameValue(eqIf("x", "x"), true, "equal strings in if");
assert.sameValue(neIf(htmldda, htmldda), false, "IsHTMLDDA with itself");

// Mixed types still coerce, once, in order.
var log = [];
var left = { valueOf: function () { log.push("l"); return 1; } };
var right = { valueOf: function () { log.push("r"); return 1; } };
assert.sameValue(eq(left, 1), true, "object == number");
assert.sameValue(eq(1, left), true, "number == object");
assert.sameValue(eqIf(left, "1"), true, "object == string in if");
assert.sameValue(eq("1", 1), true, "string == number");
assert.sameValue(eq(true, 1), true, "boolean == number");
assert.sameValue(eq(1n, 1), true, "BigInt == number");
assert.sameValue(eq(1n, "1"), true, "BigInt == string");
assert.sameValue(eq({}, "[object Object]"), true, "object == string via toString");
log.length = 0;
assert.sameValue(eq(left, right), false, "two objects are not coerced");
assert.sameValue(log.join(), "", "valueOf not called for two objects");
assert.throws(TypeError, function () { eq({ valueOf: function () { throw new TypeError("x"); } }, 1); }, "valueOf throws");

// while condition
var n = 0, m = 3;
while (m != null && n != 3) n++;
assert.sameValue(n, 3, "while with loose comparisons");
var holder = { next: { next: null } };
var count = 0;
for (var cur = holder; cur != null; cur = cur.next) count++;
assert.sameValue(count, 2, "linked list walk with != null");
