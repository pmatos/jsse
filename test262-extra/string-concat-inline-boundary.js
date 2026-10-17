// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-applystringornumericbinaryoperator
description: >
  Concatenation keeps exact code units across the sizes at which a string
  result changes representation: short results, a result of exactly one past
  a short buffer, repeated appends, lone surrogates that must not merge or
  split, and results used as property keys and compared for equality.
---*/

function repeat(unit, count) {
  var s = "";
  for (var i = 0; i < count; i++) s = s + unit;
  return s;
}

for (var n = 0; n <= 90; n++) {
  var left = repeat("a", n);
  var joined = left + "b";
  assert.sameValue(joined.length, n + 1, "length " + n);
  assert.sameValue(joined.charAt(n), "b", "tail " + n);
  assert.sameValue(joined.slice(0, n), left, "prefix " + n);
  assert.sameValue(left.length, n, "operand unchanged " + n);
  assert.sameValue(joined === repeat("a", n) + "b", true, "equality across representations " + n);
}

var base = repeat("x", 39);
var oneMore = base + "y";
var pastBoundary = oneMore + "z";
assert.sameValue(base.length, 39);
assert.sameValue(oneMore, repeat("x", 39) + "y");
assert.sameValue(pastBoundary, repeat("x", 39) + "yz");
var sibling = oneMore + "w";
assert.sameValue(pastBoundary.charAt(40), "z", "append does not clobber");
assert.sameValue(sibling.charAt(40), "w", "sibling append keeps its own tail");
assert.sameValue(oneMore.length, 40);

var high = "\uD83D";
var low = "\uDE00";
var pair = high + low;
assert.sameValue(pair.length, 2);
assert.sameValue(pair, "\u{1F600}", "surrogates join into a pair of equal code units");
assert.sameValue(pair.codePointAt(0), 0x1F600);
var splitAgain = pair.slice(0, 1) + pair.slice(1);
assert.sameValue(splitAgain, pair);
var padded = repeat("p", 39) + high;
assert.sameValue(padded.length, 40);
var paddedJoined = padded + low;
assert.sameValue(paddedJoined.codePointAt(39), 0x1F600, "pair formed across the boundary");
assert.sameValue((repeat("p", 38) + high + low).codePointAt(38), 0x1F600);

var keys = {};
keys[repeat("k", 41) + "1"] = 1;
keys[repeat("k", 41) + "2"] = 2;
assert.sameValue(keys[repeat("k", 41) + "1"], 1, "buffered key");
assert.sameValue(keys[repeat("k", 20) + repeat("k", 21) + "2"], 2, "key built from two parts");
keys["short" + 1] = 3;
assert.sameValue(keys.short1, 3, "inline key");

var built = "";
for (var i = 0; i < 300; i++) built += String.fromCharCode(97 + (i % 26));
assert.sameValue(built.length, 300);
assert.sameValue(built.charAt(299), String.fromCharCode(97 + (299 % 26)));
assert.sameValue(built.indexOf("xyz"), 23);

assert.sameValue("" + "", "");
assert.sameValue("" + repeat("e", 50), repeat("e", 50));
assert.sameValue(repeat("e", 50) + "", repeat("e", 50));
assert.sameValue("a" + 1 + null + undefined + true, "a1nullundefinedtrue");
assert.sameValue(1 + "a" + 2, "1a2");
