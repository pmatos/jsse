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
