// A typed array's [[DefineOwnProperty]] on a canonical numeric index runs
// IntegerIndexedElementSet, which coerces the descriptor's [[Value]] with the
// abstract operation ToNumber (§7.1.4) — the same StringToNumber (§7.1.4.1)
// that backs Number(str), unary +, and arithmetic. A String value must therefore
// honour NonDecimalIntegerLiteral prefixes (0x / 0o / 0b), strip exactly the
// ECMAScript StrWhiteSpace set, and treat host-parser spellings such as "inf" as
// NaN — not defer to a naive float parse.
//
// Object.defineProperties reaches the exotic [[DefineOwnProperty]] with the raw
// (un-coerced) descriptor value, so it is the path that exercises the coercion.
// (Direct element assignment `ta[i] = s` and Reflect.set coerce earlier and were
// already correct; test262's DefineOwnProperty/set-value.js only ever defines a
// Number via Reflect.defineProperty, so the String coercion is otherwise
// unverified.) Expected values cross-checked with Node.
//
// Spec: ECMAScript
//   sec-integer-indexed-exotic-objects-defineownproperty-p-desc (§10.4.5.3)
//   sec-integerindexedelementset
//   sec-tonumber (§7.1.4), sec-stringtonumber (§7.1.4.1)

function assertEq(actual, expected, msg) {
  // Distinguish +0 from -0 as well as ordinary inequality.
  if (actual !== expected || 1 / actual !== 1 / expected) {
    throw new Test262Error(
      msg + ": expected " + expected + " but got " + actual
    );
  }
}

function assertNaN(actual, msg) {
  if (actual === actual) {
    throw new Test262Error(msg + ": expected NaN but got " + actual);
  }
}

// Define index 0 of a fresh typed array of `TA` with a raw descriptor value and
// read the element back.
function defineFirst(TA, value) {
  var ta = new TA(1);
  Object.defineProperties(ta, { 0: { value: value } });
  return ta[0];
}

// (1) NonDecimalIntegerLiteral prefixes are honoured on an integer element type.
assertEq(defineFirst(Int8Array, "0x10"), 16, "hex string -> Int8 element");
assertEq(defineFirst(Int8Array, "0o17"), 15, "octal string -> Int8 element");
assertEq(defineFirst(Int8Array, "0b101"), 5, "binary string -> Int8 element");

// (2) Exactly the ECMAScript whitespace set is trimmed before parsing.
assertEq(defineFirst(Int8Array, "  5  "), 5, "space-padded decimal -> Int8 element");
assertEq(defineFirst(Int8Array, "\t\n\r 5 "), 5, "ASCII whitespace trimmed -> Int8 element");
assertEq(defineFirst(Float64Array, " 3.5 "), 3.5, "space-padded float -> Float64 element");

// (3) The float element type also routes through ToNumber, so the same prefix
//     and host-spelling rules apply.
assertEq(defineFirst(Float64Array, "0x10"), 16, "hex string -> Float64 element");
assertNaN(defineFirst(Float64Array, "inf"), "'inf' is not Infinity -> Float64 element");
assertEq(defineFirst(Float64Array, "Infinity"), Infinity, "'Infinity' word -> Float64 element");

// (4) The non-String branches of ToNumber are unchanged: Number, Boolean, and
//     empty/whitespace-only strings coerce as before.
assertEq(defineFirst(Int8Array, 300), 44, "Number wraps to Int8 (ToInt8(300))");
assertEq(defineFirst(Int8Array, true), 1, "Boolean true -> 1 -> Int8 element");
assertEq(defineFirst(Float64Array, ""), 0, "empty string -> +0 -> Float64 element");

// (5) Object.defineProperty (single) already routed through the spec ToNumber;
//     both define entry points must now agree.
var single = new Int8Array(1);
Object.defineProperty(single, "0", { value: "0x10" });
assertEq(single[0], 16, "Object.defineProperty agrees with Object.defineProperties");
