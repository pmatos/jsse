/*---
description: Values accumulated by array and object literals remain reachable while later elements allocate
esid: sec-array-initializer-runtime-semantics-evaluation
features: [class, object-spread]
---*/

function allocate() {
  var values = [];
  for (var i = 0; i < 6000; i++) {
    values.push({index: i});
  }
  return values;
}

class Box {
  constructor(value) {
    this.value = value;
  }
}

var firstArrayValue = new Box("array");
var spreadResult = [
  ...[firstArrayValue],
  ...[allocate(), new Box("later")],
];

assert.sameValue(spreadResult[0], firstArrayValue);
assert.sameValue(spreadResult[0].value, "array");

var firstObjectValue = new Box("object");
var objectResult = {
  first: firstObjectValue,
  pressure: allocate(),
};

assert.sameValue(objectResult.first, firstObjectValue);
assert.sameValue(objectResult.first.value, "object");
