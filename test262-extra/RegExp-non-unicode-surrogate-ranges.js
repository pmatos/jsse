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
