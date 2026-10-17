// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-evaluation-of-property-access-with-identifier-key
description: >
  A function body that mentions many distinct property names, each several
  times and in interleaved order, reads and writes the same property for the
  same name no matter how large the body's name pool grows.
---*/

var count = 200;
var source = "var o = {};\n";
for (var i = 0; i < count; i++) {
  source += "o.n" + i + " = " + i + ";\n";
}
for (var round = 0; round < 3; round++) {
  for (var i = count - 1; i >= 0; i -= 1 + round) {
    source += "o.n" + i + " = o.n" + i + " + 1;\n";
  }
}
source += "return o;";

var o = new Function(source)();
var expected = {};
for (var i = 0; i < count; i++) expected[i] = i;
for (var round = 0; round < 3; round++) {
  for (var i = count - 1; i >= 0; i -= 1 + round) expected[i] += 1;
}
for (var i = 0; i < count; i++) {
  assert.sameValue(o["n" + i], expected[i], "n" + i);
}
assert.sameValue(Object.keys(o).length, count, "no property was created twice");
assert.sameValue(Object.keys(o)[0], "n0", "creation order");
assert.sameValue(Object.keys(o)[count - 1], "n" + (count - 1), "creation order tail");

var withShared = "var t = 0; var a = {};\n";
for (var i = 0; i < 40; i++) {
  withShared += "a.k" + (i % 7) + " = " + i + "; t += a.k" + ((i * 3) % 7) + " | 0;\n";
}
withShared += "return [t, Object.keys(a).join()];";
var result = new Function(withShared)();
var t = 0;
var a = {};
for (var i = 0; i < 40; i++) {
  a["k" + (i % 7)] = i;
  t += a["k" + ((i * 3) % 7)] | 0;
}
assert.sameValue(result[0], t, "running total");
assert.sameValue(result[1], Object.keys(a).join(), "key order");
