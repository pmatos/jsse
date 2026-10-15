/*---
description: >
  Function declarations placed after a return or yield in an async generator
  body are instantiated at function entry.
esid: sec-functiondeclarationinstantiation
info: |
  EvaluateAsyncGeneratorBody performs FunctionDeclarationInstantiation before
  the body runs, so every function declaration of the body exists before the
  first statement, wherever it appears.

  Regression test for issue #1032.
flags: [async]
---*/

async function* afterReturn() {
  return s;
  function s() {}
}

async function* afterYield() {
  yield s();
  yield s === s;
  function s() { return 9; }
}

afterReturn().next().then(function (r) {
  assert.sameValue(typeof r.value, "function", "declaration after return");
  assert.sameValue(r.done, true);
  var it = afterYield();
  return it.next().then(function (first) {
    assert.sameValue(first.value, 9, "declaration after yield used by it");
    return it.next();
  }).then(function (second) {
    assert.sameValue(second.value, true, "identity across suspension");
  });
}).then($DONE, $DONE);
