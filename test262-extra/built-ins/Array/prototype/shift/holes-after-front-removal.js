// Copyright (C) 2026 the jsse authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-array.prototype.shift
description: >
  Shifting an array with holes keeps the holes absent, and an inherited index
  property is not mistaken for an own element once the array has been shifted.
info: |
  Array.prototype.shift ( )

  7.c. Let fromPresent be ? HasProperty(O, from).
  7.e. Else, Perform ? DeletePropertyOrThrow(O, to).
---*/

var holey = [];
for (var i = 0; i < 100; i++) {
  if (i % 3 !== 0) holey[i] = i;
}
holey.length = 100;
for (var i = 0; i < 40; i++) holey.shift();
assert.sameValue(holey.length, 60);
for (var i = 0; i < 60; i++) {
  var src = i + 40;
  var present = src % 3 !== 0;
  assert.sameValue(i in holey, present, "index " + i + " presence");
  if (present) assert.sameValue(holey[i], src, "index " + i + " value");
}

Object.prototype[1] = "inherited";
try {
  var h = [];
  h[0] = "a";
  h[2] = "c";
  h.length = 3;
  assert.sameValue(h.shift(), "a");
  assert.sameValue(h.length, 2);
  assert.sameValue(h[0], "inherited", "hole at 1 reads the prototype");
  assert.sameValue(h.hasOwnProperty(0), true, "inherited value was written as own");
  assert.sameValue(h[1], "c");
} finally {
  delete Object.prototype[1];
}
