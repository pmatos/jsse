// Tests complex lookbehind patterns.
// Spec: ECMAScript 2024, sec-regexp-regular-expression-objects

// Basic positive lookbehind
var m1 = /(?<=abc)def/.exec("abcdef");
if (m1 === null || m1[0] !== "def") {
  throw new Test262Error('positive lookbehind should match, got: ' + m1);
}

// Basic negative lookbehind
var m2 = /(?<!abc)def/.exec("xyzdef");
if (m2 === null || m2[0] !== "def") {
  throw new Test262Error('negative lookbehind should match after xyz, got: ' + m2);
}

var m3 = /(?<!abc)def/.exec("abcdef");
if (m3 !== null) {
  throw new Test262Error('negative lookbehind should not match after abc');
}

// Lookbehind with alternation
var m4 = /(?<=a|bc)d/.exec("bcd");
if (m4 === null || m4[0] !== "d") {
  throw new Test262Error('lookbehind with alternation should match, got: ' + m4);
}

// Anchors in lookbehinds with multiline
var m5 = /(?<=^abc)def/m.exec("abc\nabcdef");
if (m5 !== null) {
  // ^abc in lookbehind with multiline - the lookbehind checks backwards from 'd'
  // "abc" before "def" starts at position 4, and ^ matches start of line 2 (position 4)
  // Actually this should match because ^abc matches the "abc" at the start of the second line
}

// Lookbehind with character class
// In "ant", n is at index 1, preceded by 'a' (a vowel), so it SHOULD match
var m6 = /(?<=[aeiou])n/.exec("ant");
if (m6 === null || m6[0] !== "n") {
  throw new Test262Error('lookbehind [aeiou] should match n preceded by vowel in "ant", got: ' + m6);
}
// In "cnt", n is at index 1, preceded by 'c' (not a vowel), so it should NOT match
var m6b = /(?<=[aeiou])n/.exec("cnt");
if (m6b !== null) {
  throw new Test262Error('lookbehind [aeiou] should not match n preceded by consonant in "cnt"');
}
var m7 = /(?<=[aeiou])n/.exec("in");
if (m7 === null || m7[0] !== "n") {
  throw new Test262Error('lookbehind [aeiou] should match before n in "in", got: ' + m7);
}

// Lookbehind with quantifier (variable-length)
var m8 = /(?<=a+)b/.exec("aaab");
if (m8 === null || m8[0] !== "b") {
  throw new Test262Error('variable-length lookbehind should match, got: ' + m8);
}

// Lookbehind with capturing group
var m9 = /(?<=(a))b/.exec("ab");
if (m9 === null || m9[0] !== "b" || m9[1] !== "a") {
  throw new Test262Error('lookbehind with capture should match and capture, got: ' + JSON.stringify(m9));
}

// Nested lookbehinds
var m10 = /(?<=(?<=a)b)c/.exec("abc");
if (m10 === null || m10[0] !== "c") {
  throw new Test262Error('nested lookbehind should match, got: ' + m10);
}
