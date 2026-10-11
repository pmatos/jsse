// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-applystringornumericbinaryoperator
description: >
  String + and += behave as immutable value concatenation even when the implementation extends a shared buffer in place.
---*/

// Tests that `+` / `+=` on strings behave as immutable value concatenation even
// when the implementation extends a shared buffer in place (issue #922).
// Spec: ECMAScript 2024, sec-applystringornumericbinaryoperator (string
//       concatenation), sec-ecmascript-language-types-string-type
// Concatenation yields a new String; earlier values, their copies, and values
// derived from a common prefix must never observe a later append.

function longPrefix() {
  var s = '';
  for (var i = 0; i < 200; i++) { s += 'p'; }
  return s;
}

// Two extensions of one prefix are independent.
var s = longPrefix();
var a = s + 'x';
var b = s + 'y';
assert.sameValue(a.length, 201, 'a.length');
assert.sameValue(b.length, 201, 'b.length');
assert.sameValue(a.charAt(200), 'x', 'a tail');
assert.sameValue(b.charAt(200), 'y', 'b tail');
assert.sameValue(s.length, 200, 'prefix length unchanged');
assert.sameValue(a === b, false, 'siblings differ');
assert.sameValue(a.slice(0, 200), s, 'a prefix equals s');

// Appending to a stale view after the tail moved on still gives the right value.
var base = longPrefix();
var first = base + 'A';
var second = first + 'B';
var stale = first + 'C';
assert.sameValue(first.slice(200), 'A', 'first');
assert.sameValue(second.slice(200), 'AB', 'second');
assert.sameValue(stale.slice(200), 'AC', 'stale');
assert.sameValue(base.length, 200, 'base length');

// Compound assignment on aliased variables.
var p = longPrefix();
var q = p;
p += '1';
q += '2';
assert.sameValue(p.slice(200), '1', 'p after +=');
assert.sameValue(q.slice(200), '2', 'q after +=');

// Surrogate halves joined across the boundary are plain code units.
var hi = longPrefix() + '\uD83D';
var joined = hi + '\uDE00';
assert.sameValue(joined.length, 202, 'surrogate join length');
assert.sameValue(joined.charCodeAt(200), 0xD83D, 'high surrogate kept');
assert.sameValue(joined.charCodeAt(201), 0xDE00, 'low surrogate kept');
assert.sameValue(hi.length, 201, 'lone high surrogate string untouched');
assert.sameValue(hi.charCodeAt(200), 0xD83D, 'lone high surrogate preserved');
assert.sameValue(joined.codePointAt(200), 0x1F600, 'pair forms a code point');
var lonely = hi + 'z';
assert.sameValue(lonely.charCodeAt(200), 0xD83D, 'high surrogate then BMP');
assert.sameValue(lonely.charCodeAt(201), 0x7A, 'BMP after high surrogate');

// Derived property keys and Map keys use the content, not the buffer.
var k = longPrefix();
var o = {};
o[k + 'a'] = 1;
o[k + 'b'] = 2;
assert.sameValue(o[k + 'a'], 1, 'key a');
assert.sameValue(o[k + 'b'], 2, 'key b');
assert.sameValue(Object.keys(o).length, 2, 'two keys');
var m = new Map();
m.set(k + 'a', 'A');
m.set(k + 'b', 'B');
assert.sameValue(m.get(k + 'a'), 'A', 'map a');
assert.sameValue(m.get(k + 'b'), 'B', 'map b');
assert.sameValue(m.has(k + 'c'), false, 'map missing');

// Views sharing a buffer compare by content.
var t = longPrefix();
var t1 = t + 'q';
var t2 = longPrefix() + 'q';
assert.sameValue(t1 === t2, true, 'equal content, different buffers');
assert.sameValue(t1 < t + 'r', true, 'ordering by code units');
assert.sameValue(t < t1, true, 'prefix sorts first');

// Concatenating onto the subject from inside a replace callback.
var subject = longPrefix() + '-tail';
var seen = [];
var replaced = subject.replace(/-/, function (match) {
  var extended = subject + '!';
  seen.push(extended.length);
  return '+';
});
assert.sameValue(seen[0], 206, 'callback observed extended subject');
assert.sameValue(subject.length, 205, 'subject unchanged');
assert.sameValue(replaced.slice(200), '+tail', 'replace result');

// Many appends then reads stay consistent.
var acc = '';
var parts = [];
for (var i = 0; i < 500; i++) {
  acc += String.fromCharCode(97 + (i % 26));
  if (i % 100 === 0) { parts.push(acc); }
}
assert.sameValue(acc.length, 500, 'acc length');
for (var j = 0; j < parts.length; j++) {
  assert.sameValue(acc.indexOf(parts[j]), 0, 'snapshot ' + j + ' is a prefix');
  assert.sameValue(parts[j].length, j * 100 + 1, 'snapshot ' + j + ' length');
}
