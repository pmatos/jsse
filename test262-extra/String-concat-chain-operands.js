// Tests mixed-operand string concatenation chains such as `t = t + "ab" + j`.
// Spec: ECMAScript 2024, sec-applystringornumericbinaryoperator,
//       sec-addition-operator-plus-runtime-semantics-evaluation
// The left-associative chain must evaluate each `+` separately and ToString the
// non-String operand, yielding exactly the concatenated code units.

var t = '';
for (var j = 0; j < 300; j++) { t = t + 'ab' + j; }
var expected = '';
for (var j = 0; j < 300; j++) { expected += 'ab' + String(j); }
if (t !== expected) {
  throw new Test262Error('chain mismatch: length ' + t.length + ' vs ' + expected.length);
}
if (t.length !== expected.length || t.charCodeAt(t.length - 1) !== expected.charCodeAt(expected.length - 1)) {
  throw new Test262Error('chain tail mismatch');
}

// Number, boolean, null, undefined and BigInt operands on the right.
var base = '';
for (var i = 0; i < 100; i++) { base += 'b'; }
var r = base + 1 + true + null + undefined + 10n;
if (r !== base + '1truenullundefined10') {
  throw new Test262Error('mixed operands: got ' + r.slice(100));
}

// Non-String on the left.
var l = 1 + base;
if (l.charAt(0) !== '1' || l.length !== 101) {
  throw new Test262Error('number + string: got length ' + l.length);
}

// Empty operands.
var e1 = '' + base;
var e2 = base + '';
if (e1 !== base || e2 !== base || e1.length !== 100 || e2.length !== 100) {
  throw new Test262Error('empty operand handling');
}

// Symbol operand still throws TypeError.
var threw = false;
try { base + Symbol('s'); } catch (e) { threw = e instanceof TypeError; }
if (!threw) { throw new Test262Error('string + symbol must throw TypeError'); }

// Object operands use ToPrimitive (valueOf/toString order for default hint).
var calls = [];
var obj = {
  valueOf: function () { calls.push('valueOf'); return 'V'; },
  toString: function () { calls.push('toString'); return 'S'; }
};
var o = base + obj;
if (o.slice(100) !== 'V' || calls.join() !== 'valueOf') {
  throw new Test262Error('object operand: got ' + o.slice(100) + ' / ' + calls.join());
}
