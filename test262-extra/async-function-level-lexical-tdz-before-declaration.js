// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-functiondeclarationinstantiation
description: >
  A `let`, `const`, class or `using` declared at the top level of an async
  function body is uninitialised until its declaration runs, also when the
  function awaits before that declaration.
info: |
  FunctionDeclarationInstantiation ( func, argumentsList )

  [...]
  34. Let lexDeclarations be the LexicallyScopedDeclarations of code.
  35. For each element d of lexDeclarations, do
    a. For each element dn of the BoundNames of d, do
      i. If IsConstantDeclaration of d is true, then
        1. Perform ! lexEnv.CreateImmutableBinding(dn, true).
      ii. Else,
        1. Perform ! lexEnv.CreateMutableBinding(dn, false).

  Declarative Environment Records: GetBindingValue ( N, S )

  2. If the binding for N in envRec has not yet been initialized, throw a ReferenceError exception.

  Declarative Environment Records: SetMutableBinding ( N, V, S )

  3. Else if the binding for N in envRec has not yet been initialized, then
    a. Throw a ReferenceError exception.
flags: [async]
includes: [asyncHelpers.js]
features: [explicit-resource-management]
---*/

function name(fn) {
  try {
    fn();
    return "no error";
  } catch (e) {
    return e.constructor.name;
  }
}

// A catch around the read keeps the check inside the function, whose later `await` is
// what puts the body on the state machine when the bytecode compiler cannot host it.
async function letRead() {
  try { x; } catch (e) { var r = e.name; }
  await 0;
  let x = 1;
  return r + x;
}

async function probes() {
  var seen = [
    name(() => x),
    name(() => { x = 1; }),
    name(() => typeof x),
    name(() => c),
    name(() => C),
    name(() => u),
    name(() => [p, q])
  ];
  await 0;
  let x = 1;
  const c = 2;
  class C {}
  using u = null;
  let [p, q] = [3, 4];
  return seen;
}

async function awaitUsingProbe() {
  var seen = name(() => d);
  await 0;
  await using d = null;
  return seen;
}

async function forAwaitProbe() {
  var seen = name(() => z);
  for await (var item of []) { }
  let z = 1;
  return seen;
}

async function* generatorProbe() {
  var seen = name(() => g);
  await 0;
  let g = 1;
  yield seen;
}

asyncTest(async function () {
  assert.sameValue(await letRead(), "ReferenceError1", "read in a catch before the declaration");
  assert.sameValue(
    (await probes()).join(),
    "ReferenceError,ReferenceError,ReferenceError,ReferenceError,ReferenceError,ReferenceError,ReferenceError",
    "reads, writes and typeof of every kind of function-level lexical declaration"
  );
  assert.sameValue(await awaitUsingProbe(), "ReferenceError", "await using");
  assert.sameValue(await forAwaitProbe(), "ReferenceError", "async function with for await");
  assert.sameValue((await generatorProbe().next()).value, "ReferenceError", "async generator");
});
