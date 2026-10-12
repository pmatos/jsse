/*---
description: >
  Binary operators over two Numbers, including operands folded into the
  operator as numeric literals, agree with the same operators evaluated by the
  generic path over edge values (signed zeros, NaN, infinities, int32/uint32
  boundaries, fractions).
info: |
  sec-applystringornumericbinaryoperator, sec-numeric-types-number-add and its
  siblings, sec-toint32, sec-touint32, sec-islessthan, sec-isstrictlyequal.
  A function whose body contains an object literal is not compiled to
  bytecode, so it provides the generic oracle for the compiled variants.
esid: sec-applystringornumericbinaryoperator
---*/

var ops = ["+", "-", "*", "/", "%", "**", "<<", ">>", ">>>", "&", "|", "^",
  "<", ">", "<=", ">=", "==", "!=", "===", "!=="];
var literals = ["0", "-0", "1", "-1", "2147483648", "-2147483649", "4294967296",
  "1e21", "NaN", "Infinity", "-Infinity", "0.5", "-7.25", "31", "33"];

ops.forEach(function (op) {
  var generic = new Function("a", "b", "({}); return a " + op + " b;");
  var compiled = new Function("a", "b", "return a " + op + " b;");
  literals.forEach(function (lhsSource) {
    var lhs = (0, eval)("(" + lhsSource + ")");
    var withLiteral = new Function("a", "return a " + op + " (" + lhsSource + ");");
    var withFreeName = new Function("return globalThis.__lhs " + op + " (" + lhsSource + ");");
    literals.forEach(function (rhsSource) {
      var rhs = (0, eval)("(" + rhsSource + ")");
      var expected = generic(lhs, rhs);
      assert.sameValue(compiled(lhs, rhs), expected, lhsSource + " " + op + " " + rhsSource);
      var literalForm = new Function("a", "return a " + op + " (" + rhsSource + ");");
      assert.sameValue(literalForm(lhs), expected, "literal rhs: " + lhsSource + " " + op + " " + rhsSource);
      globalThis.__lhs = lhs;
      var freeForm = new Function("return globalThis.__lhs " + op + " (" + rhsSource + ");");
      assert.sameValue(freeForm(), expected, "member lhs: " + lhsSource + " " + op + " " + rhsSource);
    });
  });
});
delete globalThis.__lhs;

assert.sameValue((function (a) { return a | 0; })(4294967295), -1);
assert.sameValue((function (a) { return a >>> 0; })(-1), 4294967295);
assert.sameValue((function (a) { return a << 32; })(1), 1);
assert.sameValue((function (a) { return a % 0; })(5), NaN);
assert.sameValue(1 / (function (a) { return a * 1; })(-0), -Infinity);
assert.sameValue((function (a) { return a === NaN; })(NaN), false);
