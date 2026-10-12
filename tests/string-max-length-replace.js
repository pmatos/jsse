// Engine-wide maximum string length (issue #940), replace paths: a result
// built by replace, replaceAll, RegExp.prototype[Symbol.replace] or
// GetSubstitution that passes 2^29 - 24 code units throws
// RangeError: Invalid string length. Kept apart from string-max-length.js to
// bound the peak memory of each process (about 1.5 GiB here).

var HALF = 268435456; // 2**28

function check(cond, msg) {
  if (!cond) throw new Error(msg);
}

function expectRangeError(label, fn) {
  var caught;
  try {
    fn();
  } catch (e) {
    caught = e;
  }
  check(caught instanceof RangeError, label + ": expected RangeError, got " + caught);
  check(caught.message === "Invalid string length", label + ": message " + caught.message);
}

// Replacing one code unit of `half` with `half`, or two matches with `half`,
// gives more than MAX code units.
var half = "x".repeat(HALF);

expectRangeError("replace", function () { return half.replace("x", half); });
expectRangeError("replaceAll", function () { return "aaa".replaceAll("a", half); });
expectRangeError("replace function result", function () {
  return half.replace("x", function () { return half; });
});
expectRangeError("replace $' substitution", function () { return half.replace("x", "$'$'"); });
expectRangeError("regexp replace global", function () { return "aaa".replace(/a/g, half); });
expectRangeError("regexp replace function", function () {
  return "aaa".replace(/a/g, function () { return half; });
});

check("abc".replace("b", "$&$&") === "abbc", "usable after RangeError");
