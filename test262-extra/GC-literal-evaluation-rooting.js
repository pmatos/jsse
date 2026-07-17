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

// The Box values must be reachable ONLY through the in-flight literal
// accumulator while allocate() applies GC pressure. Do not hoist them into
// top-level bindings: the global environment is always traced as a GC root, so
// a `var box = new Box(...)` would keep the object alive regardless of the
// accumulator rooting under test, making this a no-op that passes even if that
// rooting is removed.
var spreadResult = [
  ...[new Box("array")],
  ...[allocate(), new Box("later")],
];

assert.sameValue(spreadResult[0].value, "array");

var objectResult = {
  first: new Box("object"),
  pressure: allocate(),
};

assert.sameValue(objectResult.first.value, "object");
