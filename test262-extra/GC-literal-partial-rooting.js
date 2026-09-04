// Partially evaluated literals keep earlier values alive across later fields.
// Spec: ECMAScript 2026, sec-object-initializer-runtime-semantics-evaluation
// and sec-array-initializer-runtime-semantics-evaluation

function allocateValue() {
  var values = [];
  for (var i = 0; i < 10000; i++) {
    values.push({ index: i });
  }
  return { last: true };
}

var object = {
  first: { marker: 1 },
  last: allocateValue(),
};
if (object.first.marker !== 1 || object.last.last !== true) {
  throw new Test262Error("object literal lost a partially evaluated value");
}

var array = [{ marker: 2 }, allocateValue()];
if (array[0].marker !== 2 || array[1].last !== true) {
  throw new Test262Error("array literal lost a partially evaluated value");
}
