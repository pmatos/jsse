// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-applystringornumericbinaryoperator
description: >
  Left-associative string concatenation chains with mixed operand types evaluate each + separately and ToString the non-String operand.
---*/

// Tests mixed-operand string concatenation chains such as `t = t + "ab" + j`.
// Spec: ECMAScript 2024, sec-applystringornumericbinaryoperator,
//       sec-addition-operator-plus-runtime-semantics-evaluation
// The left-associative chain must evaluate each `+` separately and ToString the
// non-String operand, yielding exactly the concatenated code units.

var t = '';
for (var j = 0; j < 300; j++) { t = t + 'ab' + j; }
var expected = '';
for (var j = 0; j < 300; j++) { expected += 'ab' + String(j); }
assert.sameValue(t, expected, 'chain');

// Number, boolean, null, undefined and BigInt operands on the right.
var base = '';
for (var i = 0; i < 100; i++) { base += 'b'; }
assert.sameValue(base + 1 + true + null + undefined + 10n, base + '1truenullundefined10', 'mixed operands');

// Non-String on the left.
var l = 1 + base;
assert.sameValue(l.charAt(0), '1', 'number + string head');
assert.sameValue(l.length, 101, 'number + string length');

// Empty operands.
assert.sameValue('' + base, base, 'empty left');
assert.sameValue(base + '', base, 'empty right');
assert.sameValue('' + 1, '1', 'empty left, number right');

// Symbol operand still throws TypeError.
assert.throws(TypeError, function () { base + Symbol('s'); }, 'string + symbol');

// Object operands use ToPrimitive (valueOf first for the default hint).
var calls = [];
var obj = {
  valueOf: function () { calls.push('valueOf'); return 'V'; },
  toString: function () { calls.push('toString'); return 'S'; }
};
assert.sameValue((base + obj).slice(100), 'V', 'object operand');
assert.sameValue(calls.join(), 'valueOf', 'ToPrimitive order');

// Template literals starting with a substitution extend the string and leave
// earlier values untouched.
var acc = '';
var snapshots = [];
for (var k = 0; k < 200; k++) {
  acc = `${acc}${k},`;
  if (k % 50 === 0) { snapshots.push(acc); }
}
var tplExpected = '';
for (var k = 0; k < 200; k++) { tplExpected += k + ','; }
assert.sameValue(acc, tplExpected, 'template accumulation');
assert.sameValue(tplExpected.indexOf(snapshots[2]), 0, 'template snapshot is a prefix');
var sib1 = `${acc}x`;
var sib2 = `${acc}y`;
assert.sameValue(sib1.slice(-1) + sib2.slice(-1), 'xy', 'template siblings');
assert.sameValue(`${acc}`, acc, 'single substitution');
assert.sameValue(`${1}${acc}`.slice(0, 3), '10,', 'non-string first substitution');
