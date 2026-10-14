// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-json.stringify
description: >
  The gap is the first 10 code units of a String space argument, whatever
  those code units are, including non-ASCII text and lone surrogates.
---*/

var gapOf = function(space) {
  return JSON.stringify([1], null, space).split("\n")[1].slice(0, -1);
};

assert.sameValue(gapOf("aaaaaaaaa€"), "aaaaaaaaa€", "10 code units ending in a 3-byte code point");
assert.sameValue(gapOf("aaaaaaaaa€ZZZ"), "aaaaaaaaa€", "truncated after a multi-byte code point");
assert.sameValue(gapOf("€€€€€€€€€€€"),
  "€€€€€€€€€€", "eleven BMP characters");
assert.sameValue(gapOf("\ud800"), "\ud800", "lone surrogate gap");
assert.sameValue(gapOf("a\ud800b"), "a\ud800b", "lone surrogate between text");
assert.sameValue(gapOf("12345678😀X"), "12345678😀", "surrogate pair filling the gap");
assert.sameValue(gapOf("123456789😀"), "123456789\ud83d", "gap ends inside a surrogate pair");

assert.sameValue(
  JSON.stringify({ a: [1] }, null, "€"),
  '{\n€"a": [\n€€1\n€]\n}',
  "nested indentation");
