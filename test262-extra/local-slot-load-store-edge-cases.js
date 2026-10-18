/*---
description: Function-local loads and stores keep TDZ, const, rooting and value semantics when served from a slot
info: |
  9.1.1.1.6 GetBindingValue / 9.1.1.1.5 SetMutableBinding of a declarative Environment Record
  6.1.6.1 The Number Type (NaN, infinities, signed zero)

  Reading an uninitialized let binding throws a ReferenceError, assigning a const throws a
  TypeError, a local that holds an object keeps it alive across a collection, and Number
  values move between locals unchanged.
esid: sec-declarative-environment-records
features: [Reflect]
includes: [compareArray.js]
---*/

function tdzRead() {
  var before;
  try {
    before = x;
  } catch (e) {
    before = e;
  }
  let x = 1;
  return before;
}
assert.sameValue(tdzRead() instanceof ReferenceError, true, "read before let initialization");

function tdzWrite() {
  try {
    y = 5;
  } catch (e) {
    return e;
  }
  let y = 1;
}
assert.sameValue(tdzWrite() instanceof ReferenceError, true, "write before let initialization");

function constWrite() {
  const c = 1;
  try {
    c = 2;
  } catch (e) {
    return [e instanceof TypeError, c];
  }
}
assert.compareArray(constWrite(), [true, 1], "const assignment throws and keeps the value");

function numbersMove() {
  var a = -0, b = NaN, c = Infinity, d = 1.5;
  var t = a; a = b; b = c; c = d; d = t;
  return [Object.is(a, NaN), b === Infinity, c === 1.5, Object.is(d, -0)];
}
assert.compareArray(numbersMove(), [true, true, true, true], "Numbers move between locals bit for bit");

function objectsSurviveCollection() {
  var a = { tag: "a" }, b = { tag: "b" }, c = null;
  c = a; a = b; b = c;
  c = null;
  $262.gc();
  var d = [a, b];
  a = d; d = null;
  $262.gc();
  return a[0].tag + a[1].tag;
}
assert.sameValue(objectsSurviveCollection(), "ba", "objects moved between locals stay rooted");

function overwriteReleasesOldObject() {
  var o = { v: 1 };
  var keep = o;
  o = 7;
  $262.gc();
  return keep.v + o;
}
assert.sameValue(overwriteReleasesOldObject(), 8, "overwriting a local does not free the other reference");

function stringsAndBigInts() {
  var s = "x", n = 10n, t = s, m = n;
  s = t + "y";
  n = m * 3n;
  t = null;
  return s + n;
}
assert.sameValue(stringsAndBigInts(), "xy30", "heap-owning values copy and drop correctly");

function loopCounters() {
  var i = 0, sum = 0;
  for (; i < 5; i++) { sum += i; }
  return [i, sum];
}
assert.compareArray(loopCounters(), [5, 10], "update and compound assignment on locals");

function closureSharesLocal() {
  var v = 1;
  function bump() { v = v + 1; }
  v = 10;
  bump();
  return v;
}
assert.sameValue(closureSharesLocal(), 11, "a closure and the frame see the same slot");
