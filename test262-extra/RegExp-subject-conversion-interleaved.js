/*---
esid: sec-regexpbuiltinexec
description: >
  Matching results are independent of how subjects interleave: a long subject
  re-executed between other long subjects and many short ones yields the same
  matches, indices and legacy statics as it does in isolation.
info: |
  RegExpBuiltinExec ( R, S ) is a function of R and the String S alone. The
  engine keeps converted copies of recent subjects; this exercises more
  distinct long subjects than any bounded cache holds, with short subjects
  between every call, and checks each result against a fresh computation.
---*/

function longSubject(tag) {
  var s = "";
  for (var i = 0; i < 60; i++) {
    s += "line " + tag + " " + i + " é中 𝌆;\n";
  }
  return s;
}

var subjects = [];
for (var k = 0; k < 7; k++) subjects.push(longSubject("s" + k));

var word = /[a-z]+/y;
var num = /(?!0)\d+/y;
var any = /\p{L}+/uy;
var shortTest = /\s/;

function expected(subject, re, index) {
  var fresh = new RegExp(re.source, re.flags);
  fresh.lastIndex = index;
  var m = fresh.exec(subject.slice(0));
  return m === null ? null : m[0] + "@" + m.index;
}

var checked = 0;
for (var round = 0; round < 4; round++) {
  for (var i = 0; i < subjects.length; i++) {
    var s = subjects[i];
    for (var j = 0; j < 10; j++) {
      shortTest.test("c" + j);
      shortTest.test(" ");
    }
    var pos = (round * 13 + i * 5) % 40;
    [word, num, any].forEach(function(re) {
      re.lastIndex = pos;
      var m = re.exec(s);
      var got = m === null ? null : m[0] + "@" + m.index;
      if (m !== null) {
        assert.sameValue(RegExp.input, s, "RegExp.input is the subject of the successful match");
        assert.sameValue(RegExp.lastMatch, m[0], "RegExp.lastMatch is the successful match");
      }
      assert.sameValue(got, expected(s, re, pos), "subject " + i + " round " + round + " " + re.source);
      checked++;
    });
  }
}
assert.sameValue(checked, 4 * 7 * 3);

// The same long subject through the global replace and match paths while
// other subjects are converted in between.
var a = longSubject("a");
var b = longSubject("b");
var viaMatch = a.match(/\d+/g).length;
b.replace(/\d+/g, "#");
"short".match(/o/);
assert.sameValue(a.match(/\d+/g).length, viaMatch);
assert.sameValue(a.replace(/\d+/g, "#").indexOf("#"), a.search(/\d/));
