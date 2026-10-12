/*---
description: >
  A local read as the left operand of a binary operation with a numeric
  literal stays reachable for the whole operation: its valueOf rebinds the
  local through the mapped arguments object and forces a collection before
  returning.
info: |
  sec-applystringornumericbinaryoperator (ToPrimitive of the left operand runs
  user code), sec-createmappedargumentsobject (writes through arguments[0]
  rebind the formal parameter).
esid: sec-applystringornumericbinaryoperator
flags: [noStrict]
---*/

var capturedArguments;
var valueOfRan = 0;

function makeObject() {
  return {
    valueOf: function () {
      capturedArguments[0] = 0;
      $262.gc();
      $262.gc();
      valueOfRan++;
      return 5;
    },
  };
}

function subject(o) {
  o = makeObject();
  capturedArguments = arguments;
  return o | 0;
}

function subjectAdd(o) {
  o = makeObject();
  capturedArguments = arguments;
  return o + 1;
}

assert.sameValue(subject(0), 5);
assert.sameValue(valueOfRan, 1);
assert.sameValue(subjectAdd(0), 6);
assert.sameValue(valueOfRan, 2);
