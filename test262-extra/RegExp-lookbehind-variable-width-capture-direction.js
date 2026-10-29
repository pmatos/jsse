/*---
esid: sec-runtime-semantics-compileassertion-node
description: >
  A lookbehind evaluates right-to-left (direction -1): inside it the first
  alternative that lets the assertion succeed wins, a greedy loop captures the
  longest run ending at the assertion, and the assertion never backtracks into
  its captures, for numbered and named groups alike.
info: |
  RS: CompileAssertion
  Assertion :: (?<= Disjunction )
  3. Let m be CompileSubpattern of Disjunction with arguments rer and backward.
  RS: CompileSubpattern, Disjunction :: Alternative | Disjunction
  (the left alternative is tried first; the right one only if it fails)
  RS: BackreferenceMatcher
  (a backreference to a capture that did not participate matches the empty string)
features: [regexp-lookbehind, regexp-named-groups]
---*/

assert.compareArray("xabcd".match(/(?<=(a|ab|abc))d/), ["d", "abc"], "alternation, only abc fits");
assert.compareArray("abcd".match(/(?<=(a|bc|abc))d/), ["d", "bc"], "first fitting alternative wins");
assert.compareArray("abcdbc".match(/(?<=(a|bc|abc))d/), ["d", "bc"], "first fitting alternative wins, longer input");
assert.compareArray("ABCD".match(/(?<=(a|bc|abc))d/i), ["D", "BC"], "ignoreCase");

assert.compareArray("abcabc".match(/(?<=(a|bc|abc)).\1/), ["abc", "bc"], "backref reads the right-to-left alternative");

assert.compareArray("abcdd".match(/(?<=([a-c]+))d\1?/), ["d", "abc"], "optional backref outside, greedy RTL capture");
assert.compareArray("abcdabc".match(/(?<=([a-c]+))d\1?/), ["dabc", "abc"], "optional backref outside matches the whole capture");
assert.compareArray("abcd".match(/(?<=([a-c]+))d\1*/), ["d", "abc"], "starred backref outside");
assert.compareArray("abcdabcabc".match(/(?<=([a-c]+))d(?:\1){2}/), ["dabcabc", "abc"], "counted backref group outside");

assert.compareArray("abcd".match(/(?<=(?<x>[a-c]+))d\k<x>?/), ["d", "abc"], "named capture, optional named backref");
assert.compareArray("abcdabc".match(/(?<=(?<x>[a-c]+))d\k<x>/), ["dabc", "abc"], "named capture, named backref");
assert.sameValue("abcd".match(/(?<=(?<x>[a-c]+))d\k<x>?/).groups.x, "abc", "groups.x");

assert.compareArray("1053".match(/(?<=(\d+)(\d+))$/), ["", "1", "053"], "two greedy captures split right-to-left");
assert.compareArray("aab".match(/(?<=\1(a))b/), ["b", "a"], "backref inside the lookbehind precedes its group");
assert.compareArray("xd".match(/(?<!(a|bc))d/), ["d", undefined], "negative lookbehind leaves captures undefined");

assert.compareArray("foo bar".match(/(?<=(\w+))\s(\w+)/), [" bar", "foo", "bar"], "group after a lookbehind capture keeps its number");
assert.compareArray("foo bar".match(/(?<=(\w+))\s(?<y>\w+)/), [" bar", "foo", "bar"], "named group after a lookbehind capture");
assert.sameValue("foo bar".match(/(?<=(\w+))\s(?<y>\w+)/).groups.y, "bar", "groups.y after a lookbehind capture");
assert.compareArray("aabcc".match(/(?<=(a+))b(?<n>c)\k<n>/), ["bcc", "aa", "c"], "named backref outside, named group outside");
assert.compareArray("aabaa".match(/(?<=(a*))b\1/), ["baa", "aa"], "star capture, backref outside");
assert.compareArray("abcd".match(/.*(?<=(ab|abc|c))(.*)/), ["abcd", "abc", "d"], "outer greedy loop backtracks across the lookbehind");
assert.compareArray("xxabxabc".match(/.*(?<=(b|abc))(.*)/), ["xxabxabc", "abc", ""], "outer loop, first fitting alternative");
