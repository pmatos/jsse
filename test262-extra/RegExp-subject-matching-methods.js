// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-regexpbuiltinexec
description: >
  Exec, global match, and string replacement preserve original UTF-16 capture
  text and positions across Unicode and non-Unicode matching semantics.
info: |
  RegExpBuiltinExec reports capture substrings and indices in the original S.
  RegExp.prototype [ @@match ] and [ @@replace ] advance empty matches with
  AdvanceStringIndex, and exec reads the matcher after ToLength(lastIndex).
includes: [compareArray.js]
features: [regexp-match-indices, regexp-named-groups, regexp-duplicate-named-groups, regexp-lookbehind, regexp-unicode-property-escapes]
flags: [noStrict]
---*/

var pua = "\u{F0000}";
var lone = "\ud800";
var high = pua.charAt(0);
var low = pua.charAt(1);
// source, flags, subject, captures, indices, global matches, template, replacement
var cases = [
  ["(?<part>.)(\\ud800)", "u", "!" + pua + lone + "?",
    [pua + lone, pua, lone], [[1, 4], [1, 3], [3, 4]], [pua + lone],
    "<$<part>:$2>", "!<" + pua + ":" + lone + ">?"],
  ["(.)", "u", "\u{1F600}" + pua + lone,
    ["\u{1F600}", "\u{1F600}"], [[0, 2], [0, 2]], ["\u{1F600}", pua, lone],
    "<$1>", "<\u{1F600}><" + pua + "><" + lone + ">"],
  ["(.)", "", pua + lone,
    [high, high], [[0, 1], [0, 1]], [high, low, lone],
    "<$1>", "<" + high + "><" + low + "><" + lone + ">"],
  ["(\\p{Cs})", "u", pua + lone,
    [lone, lone], [[2, 3], [2, 3]], [lone], "<$1>", pua + "<" + lone + ">"],
  ["(\\ud800)\\1", "u", pua + lone + lone,
    [lone + lone, lone], [[2, 4], [2, 3]], [lone + lone], "<$1>", pua + "<" + lone + ">"],
  ["(?<=(\\ud800)\\1)x", "u", pua + lone + lone + "x",
    ["x", lone], [[4, 5], [3, 4]], ["x"], "<$1>", pua + lone + lone + "<" + lone + ">"],
  ["(a){0}(b)", "", "b", ["b", undefined, "b"],
    [[0, 1], undefined, [0, 1]], ["b"], "$1<$2>", "<b>"],
  ["(a|(b))+", "", "ba", ["ba", "a", undefined],
    [[0, 2], [1, 2], undefined], ["ba"], "<$1:$2>", "<a:>"],
  ["(?:(?<part>a)|(?<part>b))", "", "ab", ["a", "a", undefined],
    [[0, 1], [0, 1], undefined], ["a", "b"], "<$<part>>", "<a><b>"]
];

for (var i = 0; i < cases.length; i++) {
  var c = cases[i];
  var label = "case " + i;
  var result = new RegExp(c[0], "d" + c[1]).exec(c[2]);
  assert.compareArray(result, c[3], label + " exec captures");
  assert.sameValue(result.index, c[4][0][0], label + " exec index");
  assert.sameValue(result.input, c[2], label + " original input");
  assert.sameValue(result.indices.length, c[4].length, label + " indices length");
  for (var j = 0; j < c[4].length; j++) {
    if (c[4][j] === undefined) {
      assert.sameValue(result.indices[j], undefined, label + " unmatched indices " + j);
    } else {
      assert.compareArray(result.indices[j], c[4][j], label + " UTF-16 indices " + j);
    }
  }
  if (i === 0 || i === 8) {
    assert.sameValue(result.groups.part, c[3][1], label + " original named capture");
    assert.compareArray(result.indices.groups.part, c[4][1], label + " named indices");
  }
  var matcher = new RegExp(c[0], "g" + c[1]);
  assert.compareArray(c[2].match(matcher), c[5], label + " global match");
  assert.sameValue(matcher.lastIndex, 0, label + " match resets lastIndex");
  var replacer = new RegExp(c[0], "g" + c[1]);
  assert.sameValue(c[2].replace(replacer, c[6]), c[7], label + " string replacement");
  assert.sameValue(replacer.lastIndex, 0, label + " replace resets lastIndex");
}

var duplicate = /(?:(?<part>a)|(?<part>b))/d.exec("b");
assert.sameValue(duplicate[1], undefined, "unselected duplicate capture is absent");
assert.sameValue(duplicate.groups.part, "b", "second duplicate resolves original name");
assert.compareArray(duplicate.indices.groups.part, [0, 1], "second duplicate indices");

var sticky = /x/gy;
sticky.lastIndex = 1;
assert.sameValue(sticky.exec("x!x"), null, "sticky failure does not search ahead");
assert.sameValue(sticky.lastIndex, 0, "sticky exec failure resets lastIndex");
assert.compareArray("x!x".match(sticky), ["x"], "sticky global match stops at gap");
assert.sameValue("x!x".replace(sticky, "_"), "_!x", "sticky replacement stops at gap");
var empty = /(?:)/gu;
assert.compareArray((pua + lone).match(empty), ["", "", ""], "Unicode empty progress");
assert.sameValue(empty.lastIndex, 0, "empty global match resets lastIndex");
assert.sameValue((pua + lone).replace(empty, "|"), "|" + pua + "|" + lone + "|",
  "replacement advances over original code points");

var recompiled = /x/d;
recompiled.exec("x");
var coercions = 0;
recompiled.lastIndex = {
  valueOf: function () {
    coercions++;
    recompiled.compile("(\\ud800)", "duy");
    return 2;
  }
};
var changed = recompiled.exec(pua + lone);
assert.sameValue(coercions, 1, "lastIndex is coerced once");
assert.sameValue(changed[1], lone, "matcher reflects compile during ToLength");
assert.compareArray(changed.indices[0], [2, 3], "recompiled UTF-16 positions");
assert.sameValue(recompiled.lastIndex, 3, "recompiled sticky flags apply immediately");
