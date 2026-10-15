/*---
description: >
  Function declarations placed after a yield or return in a generator body are
  instantiated at function entry.
esid: sec-functiondeclarationinstantiation
info: |
  EvaluateGeneratorBody performs FunctionDeclarationInstantiation before
  creating the generator's execution state, so every function declaration of the
  body exists when the first statement runs.

  Regression test for issue #1032.
---*/

function* usedBeforeDeclaration() {
  yield s();
  function s() { return 42; }
}
assert.sameValue(usedBeforeDeclaration().next().value, 42);

function* identityAcrossYield() {
  var before = s;
  yield 1;
  yield before === s;
  function s() {}
}
var it = identityAcrossYield();
it.next();
assert.sameValue(it.next().value, true, "identity across yield");

function* afterReturn() {
  return typeof s;
  function s() {}
}
assert.sameValue(afterReturn().next().value, "function");

function* duplicateLastWins() {
  yield s();
  function s() { return 1; }
  function s() { return 2; }
}
assert.sameValue(duplicateLastWins().next().value, 2);
