/*---
description: >
  Evaluated binary-expression operands remain reachable while later evaluation
  and coercion trigger garbage collection.
esid: sec-evaluatestringornumericbinaryexpression
info: |
  EvaluateStringOrNumericBinaryExpression retains lVal while it evaluates rVal,
  then passes both values to ApplyStringOrNumericBinaryOperator. For addition,
  that operation coerces lVal before rVal. An implementation must therefore
  keep both object operands reachable until their respective ToPrimitive calls
  complete, even when user code triggers garbage collection between those
  specification steps.
---*/

function leftOperand() {
  return {
    valueOf: function () {
      return 41;
    },
  };
}

function collectAndReturnOne() {
  $262.gc();
  return 1;
}

assert.sameValue(
  leftOperand() + collectAndReturnOne(),
  42,
  "lVal survives evaluation of rVal"
);

function coercingLeftOperand() {
  return {
    valueOf: function () {
      $262.gc();
      return 1;
    },
  };
}

function rightOperand() {
  return {
    valueOf: function () {
      return 41;
    },
  };
}

assert.sameValue(
  coercingLeftOperand() + rightOperand(),
  42,
  "rVal survives ToPrimitive(lVal)"
);
