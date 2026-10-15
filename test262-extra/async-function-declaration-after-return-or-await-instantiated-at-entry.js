/*---
description: >
  Function declarations placed after a return or await in an async function
  body are instantiated at function entry.
esid: sec-functiondeclarationinstantiation
info: |
  FunctionDeclarationInstantiation step 36 initializes every function in
  functionsToInitialize, taken from the complete body's VarScopedDeclarations,
  before the body is evaluated; EvaluateAsyncFunctionBody performs it before
  running the body. A declaration's position after a return does not matter.

  Regression test for issue #1032: a state-machine fragment that never ran
  never instantiated its declarations.
flags: [async]
---*/

async function afterReturn() {
  return s;
  function s() {}
}

async function afterAwait() {
  await 0;
  return s();
  function s() { return 7; }
}

async function identityAcrossAwait() {
  var before = s;
  await 0;
  return before === s;
  function s() {}
}

async function duplicateLastWins() {
  await 0;
  return s();
  function s() { return 1; }
  function s() { return 2; }
}

async function nonSimpleParameters(x = 1) {
  await 0;
  return typeof s + x;
  function s() {}
}

async function strictBody() {
  "use strict";
  await 0;
  return s();
  function s() { return this === undefined; }
}

async function shadowsParameter(s) {
  await 0;
  return typeof s;
  function s() {}
}

async function shadowsVar() {
  var s = 1;
  await 0;
  return typeof s;
  function s() {}
}

async function captures() {
  var x = 10;
  await 0;
  return s();
  function s() { return x; }
}

Promise.all([
  afterReturn(),
  afterAwait(),
  identityAcrossAwait(),
  duplicateLastWins(),
  nonSimpleParameters(),
  strictBody(),
  shadowsParameter(5),
  shadowsVar(),
  captures(),
]).then(function (r) {
  assert.sameValue(typeof r[0], "function", "declaration after return");
  assert.sameValue(r[1], 7, "declaration after await");
  assert.sameValue(r[2], true, "identity across suspension");
  assert.sameValue(r[3], 2, "last duplicate wins");
  assert.sameValue(r[4], "function1", "non-simple parameters");
  assert.sameValue(r[5], true, "strict body");
  assert.sameValue(r[6], "function", "function overrides parameter");
  assert.sameValue(r[7], "number", "var initializer runs after the function is instantiated");
  assert.sameValue(r[8], 10, "closure captures");
}).then($DONE, $DONE);
