/*---
description: >
  Assignment forms that reach PutValue share the same OrdinarySet semantics,
  including ArraySetLength effects without replacing the assignment
  expression's result with the coerced length.
info: |
  PutValue (sec-putvalue) calls the base object's [[Set]] with the Reference's
  receiver and ignores the Boolean result unless a strict Reference needs to
  throw. Assignment evaluation
  (sec-assignment-operators-runtime-semantics-evaluation) returns the computed
  right-hand value after PutValue. ArraySetLength may coerce that value for the
  stored length, but does not change the assignment expression result.
esid: sec-assignment-operators-runtime-semantics-evaluation
---*/

var simpleArray = [1, 2, 3];
var simpleRhs = {
  valueOf: function () {
    return 1;
  },
};
var simpleResult = (simpleArray.length = simpleRhs);

assert.sameValue(simpleResult, simpleRhs, "simple assignment returns its uncoerced RHS");
assert.sameValue(simpleArray.length, 1, "simple assignment still runs ArraySetLength");
assert.sameValue(simpleArray[1], undefined, "ArraySetLength removes truncated elements");

var logicalArray = [1, 2, 3];
var logicalRhs = {
  valueOf: function () {
    return 1;
  },
};
var logicalResult = (logicalArray.length &&= logicalRhs);

assert.sameValue(logicalResult, logicalRhs, "logical assignment returns its uncoerced RHS");
assert.sameValue(logicalArray.length, 1, "logical assignment still runs ArraySetLength");
assert.sameValue(logicalArray[1], undefined, "logical ArraySetLength removes truncated elements");

var compoundArray = [1, 2, 3];
var compoundResult = (compoundArray.length -= 1);

assert.sameValue(compoundResult, 2, "compound assignment returns its computed value");
assert.sameValue(compoundArray.length, 2, "compound assignment still runs ArraySetLength");
assert.sameValue(compoundArray[2], undefined, "compound ArraySetLength removes truncated elements");

var setterReceiver;
Object.defineProperty(Number.prototype, "ordinarySetReceiverProbe", {
  configurable: true,
  get: function () {
    return 0;
  },
  set: function (value) {
    "use strict";
    setterReceiver = this;
    assert.sameValue(value, 9, "the inherited setter receives the logical RHS");
  },
});

var primitiveBase = 7;
var primitiveResult = (primitiveBase.ordinarySetReceiverProbe ||= 9);

assert.sameValue(primitiveResult, 9, "primitive logical assignment returns its RHS");
assert.sameValue(
  setterReceiver,
  primitiveBase,
  "PutValue passes the original primitive as the inherited setter receiver"
);
delete Number.prototype.ordinarySetReceiverProbe;
