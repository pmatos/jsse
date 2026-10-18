/*---
description: Binary operations, comparisons and assignments between two function-local names keep evaluation order and operand semantics
info: |
  13.15.2 Runtime Semantics: Evaluation of AssignmentExpression
  13.15.3 ApplyStringOrNumericBinaryOperator
  7.2.14 IsLessThan, 7.2.15 IsLooselyEqual, 7.2.16 IsStrictlyEqual
  9.1.1.1.6 GetBindingValue (TDZ ReferenceError)

  Both operands of `a op b` are read left to right before the operator runs, ToPrimitive
  of the left operand precedes ToPrimitive of the right, a TDZ read throws before anything
  after it runs, and `a = b` of a const target throws a TypeError.
esid: sec-assignment-operators-runtime-semantics-evaluation
features: [BigInt, Symbol]
includes: [compareArray.js]
---*/

function numbers() {
  var a = 7, b = 3, r = [];
  r.push(a + b, a - b, a * b, a / b, a % b, a ** b, a << b, a >> b, a >>> b, a & b, a | b, a ^ b);
  r.push(a < b, a > b, a <= b, a >= b, a == b, a != b, a === b, a !== b);
  return r;
}
assert.compareArray(
  numbers(),
  [10, 4, 21, 7 / 3, 1, 343, 56, 0, 0, 3, 7, 4, false, true, false, true, false, true, false, true],
  "Number pairs"
);

function nonNumbers() {
  var s = "x", t = "y", n = 2n, m = 3n, u, nul = null, r = [];
  r.push(s + t, n + m, n < m, s < t, u == nul, u === nul, nul + 1);
  return r;
}
assert.compareArray(nonNumbers(), ["xy", 5n, true, true, true, false, 1], "mixed operand types");

function nanAndZero() {
  var nan = NaN, z = 0, nz = -0, inf = Infinity, r = [];
  r.push(nan === nan, nan != nan, nan < z, Object.is(z * nz, -0), Object.is(z + nz, 0), inf - inf);
  return r;
}
var nz = nanAndZero();
assert.sameValue(nz[0], false, "NaN === NaN");
assert.sameValue(nz[1], true, "NaN != NaN");
assert.sameValue(nz[2], false, "NaN < 0");
assert.sameValue(nz[3], true, "0 * -0 is -0");
assert.sameValue(nz[4], true, "0 + -0 is +0");
assert.sameValue(nz[5], NaN, "Infinity - Infinity");

function orderOfToPrimitive() {
  var log = [];
  var a = { valueOf() { log.push("a"); return 1; } };
  var b = { valueOf() { log.push("b"); return 2; } };
  var sum = a + b;
  var lt = a < b;
  var gt = b > a;
  return [sum, lt, gt, log.join("")];
}
assert.compareArray(orderOfToPrimitive(), [3, true, true, "abab" + "ba"], "left operand converts first; > swaps for IsLessThan");

function valueOfRebindsLocals() {
  var a = { valueOf() { b = 100; return 1; } };
  var b = 2;
  return a + b;
}
assert.sameValue(valueOfRebindsLocals(), 3, "both operands are read before the left operand's ToPrimitive rebinds the right");

function tdzLeft() {
  try {
    var r = a + b;
  } catch (e) {
    return e instanceof ReferenceError;
  }
  let a = 1, b = 2;
}
assert.sameValue(tdzLeft(), true, "TDZ left operand");

function tdzRightAfterLeftSideEffect() {
  var log = [];
  var obj = { valueOf() { log.push("valueOf"); return 1; } };
  try {
    var r = obj + later;
  } catch (e) {
    log.push(e instanceof ReferenceError ? "ReferenceError" : "other");
  }
  let later = 2;
  return log.join(",");
}
assert.sameValue(tdzRightAfterLeftSideEffect(), "ReferenceError", "right operand TDZ throws before ToPrimitive of the left");

function tdzCompareJump() {
  try {
    if (a < b) { return "then"; }
  } catch (e) {
    return e instanceof ReferenceError ? "ReferenceError" : "other";
  }
  let a = 1, b = 2;
}
assert.sameValue(tdzCompareJump(), "ReferenceError", "TDZ in a fused compare-and-branch");

function moveBasics() {
  var a = 1, b = 2, c;
  c = a;
  a = b;
  b = c;
  return [a, b, c];
}
assert.compareArray(moveBasics(), [2, 1, 1], "moves between locals");

function moveFromTdz() {
  try {
    d = src;
  } catch (e) {
    return e instanceof ReferenceError;
  }
  var d;
  let src = 1;
}
assert.sameValue(moveFromTdz(), true, "move from a TDZ binding");

function moveToConst() {
  const k = 1;
  var v = 2;
  try {
    k = v;
  } catch (e) {
    return [e instanceof TypeError, k];
  }
}
assert.compareArray(moveToConst(), [true, 1], "move into a const throws and keeps the value");

function moveToTdz() {
  var v = 2;
  try {
    t = v;
  } catch (e) {
    return e instanceof ReferenceError;
  }
  let t;
}
assert.sameValue(moveToTdz(), true, "move into an uninitialized let");

function movedObjectsStayRooted() {
  var a = { tag: "a" }, b = { tag: "b" }, c = null;
  c = a; a = b; b = c; c = null;
  $262.gc();
  return a.tag + b.tag;
}
assert.sameValue(movedObjectsStayRooted(), "ba", "objects moved between locals survive a collection");

function loopWithLocalBounds(n) {
  var i = 0, s = 0, lim = n;
  for (i = 0; i < lim; i++) {
    if (i < s) { s = s + 1; } else { s = s + 2; }
  }
  return [i, s];
}
assert.compareArray(loopWithLocalBounds(5), [5, 6], "local < local branches");

function loopWithConstBound() {
  var i, t = 0;
  for (i = 0; i < 4; i++) { t = t + i; }
  while (t > 2) { t = t - 1; }
  return [i, t];
}
assert.compareArray(loopWithConstBound(), [4, 2], "local < number literal branches");

function compareJumpOperandKinds() {
  var s1 = "a", s2 = "b", nan = NaN, one = 1, big = 2n, onebig = 1n, nul = null, und, r = [];
  r.push(s1 < s2 ? 1 : 0, nan < one ? 1 : 0, nan >= one ? 1 : 0, big > onebig ? 1 : 0, one < big ? 1 : 0);
  r.push(nul == und ? 1 : 0, nul === und ? 1 : 0, one == "1" ? 1 : 0, one === "1" ? 1 : 0);
  return r;
}
assert.compareArray(compareJumpOperandKinds(), [1, 0, 0, 1, 1, 1, 0, 1, 0], "non-Number operands in fused branches");

function symbolOperandThrows() {
  var a = Symbol(), b = 1;
  try {
    return a + b;
  } catch (e) {
    return e instanceof TypeError;
  }
}
assert.sameValue(symbolOperandThrows(), true, "Symbol operand throws TypeError");

function bigintMixThrows() {
  var a = 1n, b = 1;
  try {
    return a + b;
  } catch (e) {
    return e instanceof TypeError;
  }
}
assert.sameValue(bigintMixThrows(), true, "BigInt/Number mix throws TypeError");

function captured() {
  var a = 1, b = 2;
  function bump() { a = 10; b = 20; }
  var before = a + b;
  bump();
  return [before, a + b, a < b];
}
assert.compareArray(captured(), [3, 30, true], "closure writes are seen by fused reads");

function blockScoped() {
  var out = [];
  for (let i = 0; i < 3; i++) {
    let j = i * 2;
    let k = j;
    out.push(i + j, k < i);
  }
  return out;
}
assert.compareArray(blockScoped(), [0, false, 3, false, 6, false], "block-scoped locals in fused ops");
