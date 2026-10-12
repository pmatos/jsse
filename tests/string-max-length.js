// Engine-wide maximum string length (issue #940): 2^29 - 24 code units.
// Building a longer string throws RangeError: Invalid string length instead of
// aborting the process. These cases allocate strings near the limit (512 MiB
// to 1 GiB), which the test262 runner's 512 MiB memory limit cannot hold, so
// they live here instead of test262-extra/. Peak memory is about 2 GiB; the
// replace paths are in string-max-length-replace.js.

var MAX = 536870888; // 2**29 - 24
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

var half = "x".repeat(HALF);
check(half.length === HALF, "half length");

// Each of these would produce 2 * HALF = MAX + 24 code units and fails before
// allocating the result.
expectRangeError("a + a", function () { return half + half; });
expectRangeError("a += a", function () { var s = half; s += half; });
expectRangeError("concat", function () { return half.concat(half); });
expectRangeError("template", function () { return `${half}${half}`; });
expectRangeError("template prefix", function () { return `${half}-${half}`; });
expectRangeError("join", function () { return [half, half].join(""); });
expectRangeError("join separator", function () { return [half, "", half].join("yy"); });
expectRangeError("array join, long separator", function () { return new Array(3).join(half); });
expectRangeError("typed array join", function () { return new Uint8Array(3).join(half); });
expectRangeError("toString", function () { return String([half, half]); });
expectRangeError("String.raw", function () { return String.raw({ raw: [half, half] }); });
expectRangeError("padStart", function () { return half.padStart(MAX + 1); });
expectRangeError("repeat", function () { return half.repeat(2); });

// Exactly the maximum is allowed; one more code unit is not.
var tail = "y".repeat(HALF - 24);
check(half.concat(tail).length === MAX, "concat at max");
check(`${half}${tail}`.length === MAX, "template at max");
var full = half + tail;
check(full.length === MAX, "full length " + full.length);
expectRangeError("max + 1", function () { return full + "z"; });
expectRangeError("1 + max", function () { return "z" + full; });
expectRangeError("max + number", function () { return full + 1; });
expectRangeError("number + max", function () { return 1 + full; });

// The engine keeps working after the errors.
check("ab".repeat(3) === "ababab", "usable after RangeError");
