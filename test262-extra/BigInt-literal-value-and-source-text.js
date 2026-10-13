// Copyright (C) 2026 jsse contributors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
description: >
  A BigInt literal's value is its NumericValue, computed once from the source
  text, and every evaluation of the same literal yields that value in whichever
  execution path the engine uses (compiled function body, array/object
  literal templates, script top level, direct eval, generators). The literal's
  source text is still what Function.prototype.toString reports.
esid: sec-literals-numeric-literals
info: |
  NumericLiteral :: DecimalBigIntegerLiteral
  NumericLiteral :: NonDecimalIntegerLiteral BigIntLiteralSuffix

  Static Semantics: NumericValue
    DecimalBigIntegerLiteral :: 0 BigIntLiteralSuffix -> 0
    NonDecimalIntegerLiteral Suffix -> the MV of NonDecimalIntegerLiteral
    (numeric separators do not contribute to the MV)

  Function.prototype.toString: the source text matched by the function's
  syntactic production is returned for functions defined in ECMAScript code.
features: [BigInt, numeric-separator-literal, generators]
---*/

function allForms() {
  return [
    0n, 1n, 10n, -1n,
    0x10n, 0XFFn, 0o17n, 0O17n, 0b101n, 0B101n,
    1_000_000n, 0xdead_beefn, 0b1_0_1n,
    0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFn,
    123456789012345678901234567890123456789012345678901234567890n
  ];
}

var expected = [
  BigInt(0), BigInt(1), BigInt(10), -BigInt(1),
  BigInt(16), BigInt(255), BigInt(15), BigInt(15), BigInt(5), BigInt(5),
  BigInt(1000000), BigInt(3735928559), BigInt(5),
  BigInt.asUintN(256, -1n),
  BigInt("123456789012345678901234567890123456789012345678901234567890")
];

function check(actual, label) {
  assert.sameValue(actual.length, expected.length, label + ' length');
  for (var i = 0; i < expected.length; i++) {
    assert.sameValue(typeof actual[i], 'bigint', label + ' typeof [' + i + ']');
    assert.sameValue(actual[i], expected[i], label + ' [' + i + ']');
  }
}

// Repeated evaluation of a function body gives the same values every time.
for (var round = 0; round < 3; round++) {
  check(allForms(), 'function body round ' + round);
}

// Script top level.
check([
  0n, 1n, 10n, -1n,
  0x10n, 0XFFn, 0o17n, 0O17n, 0b101n, 0B101n,
  1_000_000n, 0xdead_beefn, 0b1_0_1n,
  0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFn,
  123456789012345678901234567890123456789012345678901234567890n
], 'script top level');

// Direct eval.
check(eval('[0n, 1n, 10n, -1n, 0x10n, 0XFFn, 0o17n, 0O17n, 0b101n, 0B101n,' +
  '1_000_000n, 0xdead_beefn, 0b1_0_1n,' +
  '0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFn,' +
  '123456789012345678901234567890123456789012345678901234567890n]'), 'direct eval');

// Generator body.
function* gen() {
  yield 0n; yield 1n; yield 10n; yield -1n;
  yield 0x10n; yield 0XFFn; yield 0o17n; yield 0O17n; yield 0b101n; yield 0B101n;
  yield 1_000_000n; yield 0xdead_beefn; yield 0b1_0_1n;
  yield 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFn;
  yield 123456789012345678901234567890123456789012345678901234567890n;
}
check(Array.from(gen()), 'generator');

// A literal inside a loop evaluated many times stays the same value.
function loop() {
  var x = 0n;
  for (var i = 0; i < 50; i++) { x += 0x10n; }
  return x;
}
assert.sameValue(loop(), 800n, 'literal accumulated in a loop');
assert.sameValue(loop(), 800n, 'second call');

// Property keys written as BigInt literals use the canonical decimal string.
var o = { 0x10n: 'a', 1_0n: 'b' };
assert.sameValue(o['16'], 'a', 'hex BigInt property key');
assert.sameValue(o['10'], 'b', 'separator BigInt property key');

// Destructuring pattern keys written as BigInt literals use the same value.
var { 0x10n: patA, 0b1_0n: patB } = { 16: 'a', 2: 'b' };
assert.sameValue(patA, 'a', 'hex BigInt pattern key');
assert.sameValue(patB, 'b', 'binary BigInt pattern key');

// Zero-valued prefixed forms.
assert.sameValue(0x0n, 0n, '0x0n');
assert.sameValue(0b0n, 0n, '0b0n');
assert.sameValue(0o0n, 0n, '0o0n');
assert.sameValue(-0n, 0n, '-0n');

// Each evaluation of the literal is a BigInt primitive: mutating a variable
// that held it does not change later evaluations.
function fresh() { var v = 5n; v += 1n; return v; }
assert.sameValue(fresh(), 6n, 'first call');
assert.sameValue(fresh(), 6n, 'second call is unaffected by the first');

// Function.prototype.toString keeps the literal's source text.
function src() { return 0xFF_FFn + 1_0n; }
var text = src.toString();
assert(text.indexOf('0xFF_FFn') !== -1, 'toString keeps the hex literal text: ' + text);
assert(text.indexOf('1_0n') !== -1, 'toString keeps the separator literal text: ' + text);
var arrow = () => 0b1_0n;
assert.sameValue(arrow.toString(), '() => 0b1_0n', 'arrow toString');
