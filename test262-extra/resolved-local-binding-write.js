/*---
description: Resolved local binding writes preserve PutValue semantics
info: |
  6.2.5.6 PutValue
  10.2.1.3 OrdinaryCallEvaluateBody

  Assignment captures its Reference Record before evaluating the right-hand
  side. Mutable declarative bindings update that exact record, while immutable,
  global-object, and object environment records retain their distinct behavior.
esid: sec-putvalue
flags: [noStrict]
---*/

function writeLocals() {
  var r0 = 1;
  let r1 = 2;
  r0 = 3;
  r1 = 4;
  return r0 + r1;
}
assert.sameValue(writeLocals(), 7, "mutable local bindings are updated");

assert.throws(TypeError, function () {
  function constWrite() {
    const value = 1;
    value = 2;
  }
  constWrite();
});

var sloppyNamed = function immutableName() {
  immutableName = 1;
  return immutableName;
};
assert.sameValue(
  sloppyNamed(),
  sloppyNamed,
  "sloppy writes to a named-function binding are ignored"
);

var strictNamed = function immutableName() {
  "use strict";
  immutableName = 1;
};
assert.throws(TypeError, strictNamed);

function preserveReference() {
  var capturedLocal = 1;
  function rhs() {
    capturedLocal = 40;
    return 2;
  }
  capturedLocal = rhs();
  return capturedLocal;
}
assert.sameValue(
  preserveReference(),
  2,
  "the captured local reference is written after the RHS returns"
);

var globalWrite = 1;
globalWrite = 2;
assert.sameValue(globalWrite, 2, "the global binding is updated");
assert.sameValue(globalThis.globalWrite, 2, "the global object mirror is updated");

function globalFunctionWrite() {}
globalFunctionWrite = 3;
assert.sameValue(globalFunctionWrite, 3, "the global function binding is updated");
assert.sameValue(
  globalThis.globalFunctionWrite,
  3,
  "a global function write is mirrored to the global object"
);

var withTarget = { value: 1 };
function writeWithObject() {
  var value = 2;
  with (withTarget) {
    value = 3;
  }
  return value;
}
assert.sameValue(writeWithObject(), 2, "with does not overwrite the local binding");
assert.sameValue(withTarget.value, 3, "with writes through the object environment record");
