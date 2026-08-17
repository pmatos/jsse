// Copyright (C) 2026 jsse contributors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
description: >
  StringToBigInt is a single abstract operation shared by the BigInt
  constructor, ToBigInt (typed-array element writes, DataView.setBigInt64,
  %TypedArray%.prototype.with), and BigInt/String loose equality. Every
  consumer must agree, and the operation must implement the grammar exactly:
  the empty (or all-whitespace) String is 0n, only StrWhiteSpace is stripped,
  and numeric separators, a sign inside a NonDecimalIntegerLiteral, decimal
  points, exponents, and non-ASCII digits are all rejected.
esid: sec-stringtobigint
info: |
  7.1.14 StringToBigInt ( str )
    Let text be StringToCodePoints(str).
    Let literal be ParseText(text, StringIntegerLiteral).
    If literal is a List of errors, return undefined.
    ...

  StringIntegerLiteral :::
    StrWhiteSpace_opt
    StrWhiteSpace_opt StrIntegerLiteral StrWhiteSpace_opt
  StrIntegerLiteral ::: SignedInteger | NonDecimalIntegerLiteral
  (the [~Sep] forms — no NumericLiteralSeparator; a NonDecimalIntegerLiteral has
  no sign; a SignedInteger has no radix point or exponent.)

  7.1.13 ToBigInt ( argument ): for a String, n = StringToBigInt(prim); if n is
  undefined, throw a SyntaxError.

  7.2.15 IsLooselyEqual: BigInt x, String y -> let n be StringToBigInt(y); if n
  is undefined, return false; else compare.
features: [BigInt, TypedArray, DataView]
---*/

function Bi(str) { return BigInt(str); }

// --- Valid controls: these must keep working unchanged ---
assert.sameValue(Bi(""), 0n, 'empty string is 0n');
assert.sameValue(Bi("     "), 0n, 'all-whitespace string is 0n');
assert.sameValue(Bi("   -197   "), -197n, 'signed decimal with surrounding spaces');
assert.sameValue(Bi("0xFF"), 255n, 'hex literal');
assert.sameValue(Bi("0X1a"), 26n, 'hex literal, uppercase prefix, lowercase digits');
assert.sameValue(Bi("0o17"), 15n, 'octal literal');
assert.sameValue(Bi("0b1010"), 10n, 'binary literal');
assert.sameValue(Bi("+5"), 5n, 'leading plus is allowed for SignedInteger');
assert.sameValue(Bi("00"), 0n, 'leading zeros');

// --- Numeric separators are NOT part of the string grammar ---
assert.throws(SyntaxError, function () { Bi("1_0"); }, 'decimal separator');
assert.throws(SyntaxError, function () { Bi("0x1_0"); }, 'hex separator');
assert.throws(SyntaxError, function () { Bi("0b1_0"); }, 'binary separator');

// --- A NonDecimalIntegerLiteral has no sign (the sign must precede, and only a
//     decimal SignedInteger may carry one) ---
assert.throws(SyntaxError, function () { Bi("0x-10"); }, 'sign inside hex body');
assert.throws(SyntaxError, function () { Bi("0b-1"); }, 'sign inside binary body');
assert.throws(SyntaxError, function () { Bi("-0x10"); }, 'sign before hex prefix');

// --- No radix point, exponent, BigInt suffix, or stray digits ---
assert.throws(SyntaxError, function () { Bi("1.5"); }, 'radix point');
assert.throws(SyntaxError, function () { Bi("1e3"); }, 'exponent');
assert.throws(SyntaxError, function () { Bi("10n"); }, 'BigInt suffix');
assert.throws(SyntaxError, function () { Bi("0b12"); }, 'non-binary digit');

// --- Non-ASCII digits are rejected ---
assert.throws(SyntaxError, function () { Bi("５"); }, 'fullwidth digit five');
assert.throws(SyntaxError, function () { Bi("٥"); }, 'arabic-indic digit five');

// --- Whitespace: only StrWhiteSpace is stripped. U+FEFF (ZWNBSP) IS
//     StrWhiteSpace; U+0085 (NEL) is NOT (matches StringToNumber / node, and
//     differs from Rust's char::is_whitespace). ---
assert.sameValue(Bi("﻿1"), 1n, 'U+FEFF is StrWhiteSpace and is stripped');
assert.throws(SyntaxError, function () { Bi("1"); }, 'U+0085 (NEL) is not StrWhiteSpace');

// --- ToBigInt agreement: typed-array element write ---
(function () {
  var a = new BigInt64Array(1);
  a[0] = "";
  assert.sameValue(a[0], 0n, 'typed-array element write: "" -> 0n');
  a[0] = "0x10";
  assert.sameValue(a[0], 16n, 'typed-array element write: hex');
  assert.throws(SyntaxError, function () { a[0] = "0x-10"; }, 'typed-array element write: invalid string throws');
})();

// --- ToBigInt agreement: %TypedArray%.prototype.with ---
(function () {
  assert.sameValue(new BigInt64Array(1).with(0, "")[0], 0n, 'with(): "" -> 0n');
  assert.sameValue(new BigInt64Array(1).with(0, "5")[0], 5n, 'with(): decimal');
  assert.throws(SyntaxError, function () { new BigInt64Array(1).with(0, "1_0"); }, 'with(): separator throws');
})();

// --- ToBigInt agreement: DataView.setBigInt64 ---
(function () {
  var dv = new DataView(new ArrayBuffer(8));
  dv.setBigInt64(0, "");
  assert.sameValue(dv.getBigInt64(0), 0n, 'setBigInt64: "" -> 0n');
  assert.throws(SyntaxError, function () { dv.setBigInt64(0, "0x-10"); }, 'setBigInt64: invalid string throws');
})();

// --- IsLooselyEqual agreement: a failed StringToBigInt yields `false`, never a
//     throw, and the same whitespace/grammar rules apply ---
assert.sameValue(1n == "", false, '1n == "" (empty -> 0n)');
assert.sameValue(0n == "", true, '0n == "" (empty -> 0n)');
assert.sameValue(0n == "﻿", true, '0n == U+FEFF (whitespace -> 0n)');
assert.sameValue(0n == "", false, '0n == U+0085 (NEL not whitespace -> undefined)');
assert.sameValue(1n == "﻿1", true, '1n == U+FEFF+"1" (ZWNBSP stripped)');
assert.sameValue(1n == "1", false, '1n == U+0085+"1" (NEL not stripped -> undefined)');
assert.sameValue(10n == "1_0", false, '10n == "1_0" (separator invalid -> undefined)');
assert.sameValue(16n == "0x10", true, '16n == "0x10" (hex string)');
