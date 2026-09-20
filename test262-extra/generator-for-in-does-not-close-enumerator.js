/*---
description: >
  A generator for-in loop never calls `return` on its enumerator, however the
  loop is exited.
esid: sec-enumerate-object-properties
info: |
  EnumerateObjectProperties returns an iterator whose `throw` and `return`
  methods are null and never invoked, and ForIn/OfBodyEvaluation skips
  IteratorClose for enumerate loops. A `return` installed on
  Object.prototype or %Iterator.prototype% must therefore never run.
includes: [compareArray.js]
features: [generators, iterator-helpers]
---*/

var calls = [];
var recorder = function () {
  calls.push('return');
  throw new Test262Error('enumerator must not be closed');
};
var iteratorProto = Object.getPrototypeOf(Object.getPrototypeOf([][Symbol.iterator]()));
Object.prototype.return = recorder;
iteratorProto.return = recorder;

function* broken(o) {
  for (var k in o) {
    yield k;
    break;
  }
}
function* returning(o) {
  for (var k in o) {
    yield k;
    return 1;
  }
}
function* throwing(o) {
  try {
    for (var k in o) {
      yield k;
      throw 1;
    }
  } catch (e) {}
}
function* suspended(o) {
  for (var k in o) yield k;
}

try {
  [...broken({ a: 1, b: 2 })];
  [...returning({ a: 1, b: 2 })];
  [...throwing({ a: 1, b: 2 })];
  var it = suspended({ a: 1, b: 2 });
  it.next();
  it.return();
  it = suspended({ a: 1, b: 2 });
  it.next();
  assert.throws(Test262Error, function () {
    it.throw(new Test262Error('x'));
  });
} finally {
  delete Object.prototype.return;
  delete iteratorProto.return;
}

assert.compareArray(calls, [], 'no enumerator `return` was ever invoked');
