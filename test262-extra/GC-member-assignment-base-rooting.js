// A member-assignment Reference keeps its base alive while evaluating the RHS.
// Spec: ECMAScript 2026, sec-assignment-operators-runtime-semantics-evaluation

var observed = 0;
var prototype = {
  set value(value) {
    observed = value;
  },
};

function makeBase() {
  return Object.create(prototype);
}

function allocatingRhs() {
  var values = [];
  for (var i = 0; i < 10000; i++) {
    values.push({ index: i });
  }
  return 42;
}

makeBase().value = allocatingRhs();
if (observed !== 42) {
  throw new Test262Error("assignment base was not preserved across RHS evaluation");
}
