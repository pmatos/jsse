/*---
esid: sec-regexp.prototype.compile
description: >
  The matcher an exec uses always reflects the regexp's current [[OriginalSource]]
  and [[OriginalFlags]]: recompiling an already-executed regexp, or recompiling
  it from inside the ToLength(lastIndex) coercion that exec performs first,
  takes effect for that very exec.
info: |
  RegExp.prototype.compile ( pattern, flags )

  5. Return ? RegExpInitialize(O, P, F).

  RegExpBuiltinExec ( R, S )

  2. Let lastIndex be ℝ(? ToLength(? Get(R, "lastIndex"))).
  3. Let flags be R.[[OriginalFlags]].
  6. Let matcher be R.[[RegExpMatcher]].
flags: [noStrict]
---*/

var re = /a/;
assert.sameValue(re.exec("ba").index, 1, "initial pattern");
re.compile("b");
assert.sameValue(re.source, "b");
assert.sameValue(re.exec("ba").index, 0, "pattern after compile");
re.compile("a", "y");
assert.sameValue(re.sticky, true);
assert.sameValue(re.exec("ba"), null, "sticky after compile cannot skip the first char");
assert.sameValue(re.lastIndex, 0);
re.lastIndex = 1;
assert.sameValue(re.exec("ba").index, 1, "sticky after compile matches at lastIndex");
re.compile("a");
assert.sameValue(re.sticky, false);
assert.sameValue(re.exec("ba").index, 1, "non-sticky again after compile");

// Recompile while ToLength(lastIndex) runs: the new matcher is the one used.
var r2 = /a/;
assert.sameValue(r2.exec("zab").index, 1);
r2.lastIndex = {
  valueOf: function() {
    r2.compile("b");
    return 0;
  }
};
var m = r2.exec("zab");
assert.sameValue(m[0], "b", "matcher replaced by the coercion");
assert.sameValue(m.index, 2);

// Recompile that turns on sticky while coercing lastIndex.
var r3 = /b/;
assert.sameValue(r3.exec("ab").index, 1);
r3.lastIndex = {
  valueOf: function() {
    r3.compile("b", "y");
    return 0;
  }
};
assert.sameValue(r3.exec("ab"), null, "sticky from the recompile applies to this exec");

// A recompiled regexp stays independent of another regexp with the same text.
var shared1 = /(\d)(\d)/;
var shared2 = /(\d)(\d)/;
assert.sameValue(shared1.exec("12")[1], "1");
shared1.compile("(\\d)", "g");
assert.sameValue(shared2.exec("12")[2], "2", "the other regexp is unchanged");
assert.sameValue(shared1.exec("12").length, 2, "recompiled regexp has one group");

// Legacy statics follow the executed regexp's last match.
/(x)(y)?/.exec("zzxq");
assert.sameValue(RegExp.lastMatch, "x");
assert.sameValue(RegExp.$1, "x");
assert.sameValue(RegExp.$2, "");
assert.sameValue(RegExp.leftContext, "zz");
assert.sameValue(RegExp.rightContext, "q");
