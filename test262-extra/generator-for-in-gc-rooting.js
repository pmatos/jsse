/*---
description: >
  The object a suspended generator for-in loop enumerates stays alive across
  garbage collections that run while the generator is suspended.
esid: sec-enumerate-object-properties
info: |
  The enumerator created by ForIn/OfHeadEvaluation is held by the running
  loop; primitive RHS values are wrapped by ToObject and that fresh wrapper
  must not be collected while keys remain.
includes: [compareArray.js]
features: [generators]
---*/

function churn() {
  var junk = [];
  for (var i = 0; i < 2000; i++) junk.push({ i: i, s: 'x' + i, a: [i] });
  return junk.length;
}

function* overWrapper() {
  for (var k in 'abcd') {
    churn();
    yield k;
  }
}
assert.compareArray([...overWrapper()], ['0', '1', '2', '3'], 'String wrapper survives');

function* overFresh() {
  for (var k in { p: 1, q: 2, r: 3 }) {
    churn();
    yield k;
  }
}
var it = overFresh();
var out = [];
var step;
while (!(step = it.next()).done) {
  churn();
  out.push(step.value);
}
assert.compareArray(out, ['p', 'q', 'r'], 'unreferenced object literal survives');
