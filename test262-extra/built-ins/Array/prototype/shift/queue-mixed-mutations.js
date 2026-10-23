// Copyright (C) 2026 the jsse authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-array.prototype.shift
description: >
  Repeated shift on a long dense array, interleaved with every other kind of
  element mutation, keeps returning the first element and keeps the remaining
  elements and length consistent.
info: |
  Array.prototype.shift ( )

  5. Let first be ? Get(O, "0").
  6. Let k be 1.
  7. Repeat, while k < len,
    a. Let from be ! ToString(𝔽(k)).
    b. Let to be ! ToString(𝔽(k - 1)).
    c. Let fromPresent be ? HasProperty(O, from).
    d. If fromPresent is true, then
      i. Let fromVal be ? Get(O, from).
      ii. Perform ? Set(O, to, fromVal, true).
    e. Else,
      i. Perform ? DeletePropertyOrThrow(O, to).
  8. Perform ? DeletePropertyOrThrow(O, ! ToString(𝔽(len - 1))).
  9. Perform ? Set(O, "length", 𝔽(len - 1), true).
  10. Return first.
---*/

var a = [];
var expected = [];
for (var i = 0; i < 500; i++) {
  a.push(i);
  expected.push(i);
}

for (var round = 0; round < 400; round++) {
  assert.sameValue(a.shift(), expected.shift(), "shift " + round);
  if (round % 7 === 0) {
    a.unshift(-round);
    expected.unshift(-round);
  }
  if (round % 5 === 0) {
    a.push(round, round + 1);
    expected.push(round, round + 1);
  }
  if (round % 11 === 0) {
    assert.sameValue(a.pop(), expected.pop(), "pop " + round);
  }
  if (round % 13 === 0) {
    assert.sameValue(
      a.splice(3, 2, "s").join(),
      expected.splice(3, 2, "s").join(),
      "splice " + round
    );
  }
  if (round % 17 === 0) {
    a.length = a.length - 1;
    expected.length = expected.length - 1;
  }
  assert.sameValue(a.length, expected.length, "length " + round);
  assert.sameValue(a[0], expected[0], "head " + round);
  assert.sameValue(a[a.length - 1], expected[expected.length - 1], "tail " + round);
  assert.sameValue(a[a.length], undefined, "past the end " + round);
}

assert.sameValue(a.join(), expected.join(), "final contents");

var q = [1, 2, 3];
assert.sameValue(q.shift(), 1);
assert.sameValue(q.shift(), 2);
assert.sameValue(q.shift(), 3);
assert.sameValue(q.shift(), undefined, "empty after draining");
assert.sameValue(q.length, 0);
q.push(9);
assert.sameValue(q[0], 9, "push after draining");
assert.sameValue(q.length, 1);

var big = [];
for (var i = 0; i < 100; i++) big.push(i);
for (var i = 0; i < 90; i++) big.shift();
assert.sameValue(big.length, 10);
assert.sameValue(big.indexOf(95), 5, "indexOf after many shifts");
assert.sameValue(big.slice(2, 4).join(), "92,93");
assert.sameValue(big.reverse()[0], 99, "reverse after many shifts");
big.sort(function (x, y) { return x - y; });
assert.sameValue(big.join(), "90,91,92,93,94,95,96,97,98,99", "sort after many shifts");
