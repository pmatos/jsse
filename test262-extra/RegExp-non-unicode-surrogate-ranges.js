// Tests non-Unicode RegExp ranges over UTF-16 surrogate code units.
// Spec: ECMAScript 2026, sec-regexp.prototype-%symbol.replace%

var astral = String.fromCharCode(0xD801, 0xDC37);
var seen = [];
var escaped = astral.replace(/[\u007f-\uffff]/g, function (unit) {
  seen.push(unit.charCodeAt(0));
  return "\\u" + unit.charCodeAt(0).toString(16);
});

if (escaped !== "\\ud801\\udc37") {
  throw new Test262Error("surrogate-spanning range replacement failed: " + escaped);
}
if (seen.length !== 2 || seen[0] !== 0xD801 || seen[1] !== 0xDC37) {
  throw new Test262Error("replacement callback did not receive individual code units");
}

var captures = [];
var roundTripped = astral.replace(/([\uD800-\uDFFF])/g, function (match, capture) {
  captures.push(capture.charCodeAt(0));
  return match;
});
if (roundTripped !== astral || captures[0] !== 0xD801 || captures[1] !== 0xDC37) {
  throw new Test262Error("functional replacement did not preserve surrogate code units");
}

var high = String.fromCharCode(0xD800);
var low = String.fromCharCode(0xDFFF);
if (!/[\uD800-\uDFFF]/.test(high) || !/[\uD800-\uDFFF]/.test(low)) {
  throw new Test262Error("surrogate-only range did not match lone surrogates");
}
if (/[^\uD800-\uDFFF]/.test(high) || /[^\uD800-\uDFFF]/.test(low)) {
  throw new Test262Error("negated surrogate range matched lone surrogates");
}

// A negated class beginning with a literal hyphen followed by a
// surrogate-spanning atom must treat the leading '^' as the negation marker,
// not as the low endpoint of a range. `[^-\uD800]` means "not '-' and not
// U+D800", so it must match 'A' and reject both '-' and U+D800.
if (!/[^-\uD800]/.test("A")) {
  throw new Test262Error("negated class with leading hyphen rejected an unrelated char");
}
if (/[^-\uD800]/.test("\uD800")) {
  throw new Test262Error("negated class with leading hyphen matched the excluded surrogate");
}
if (/[^-\uD800]/.test("-")) {
  throw new Test262Error("negated class with leading hyphen matched the literal hyphen");
}
// The '^' as a genuine range endpoint mid-class must still form a range.
if (!/[a^-￿]/.test("^") || !/[a^-￿]/.test("_")) {
  throw new Test262Error("literal '^' range endpoint mid-class no longer forms a range");
}

// String (non-functional) replacement must preserve captured lone surrogates
// through numbered ($N) and named ($<name>) template substitution, just as the
// functional replacement path already does.
var loneNumbered = String.fromCharCode(0xD801).replace(/([\uD800-\uDFFF])/g, "$1");
if (loneNumbered.length !== 1 || loneNumbered.charCodeAt(0) !== 0xD801) {
  throw new Test262Error("numbered $1 substitution lost the captured lone surrogate");
}
var loneNamed = String.fromCharCode(0xD801).replace(/(?<c>[\uD800-\uDFFF])/g, "$<c>");
if (loneNamed.length !== 1 || loneNamed.charCodeAt(0) !== 0xD801) {
  throw new Test262Error("named $<c> substitution lost the captured lone surrogate");
}

// Astral characters (proper surrogate pairs) must round-trip unchanged through
// $N substitution in both /u and non-Unicode modes.
var astralUnicode = "😀".replace(/(.)/u, "$1");
if (astralUnicode !== "😀") {
  throw new Test262Error("/u $1 substitution corrupted an astral capture");
}
var astralGlobal = "😀".replace(/(😀)/g, "$1");
if (astralGlobal !== "😀") {
  throw new Test262Error("non-Unicode $1 substitution corrupted an astral capture");
}
