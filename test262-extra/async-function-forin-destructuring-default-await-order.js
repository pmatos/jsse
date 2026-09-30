/*---
description: >
  An `await` inside a destructuring default in a `for-in` head's
  Variable-kind binding suspends the async function at a real Await state --
  the caller continues synchronously and jobs already queued run before the
  function's continuation -- exactly like an `await` in a plain
  declaration's default. `ForOfHead` is the shared driver terminator for
  both `for-in` and `for-of` in this engine.
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind, lhsKind,
  labelSet [ , iteratorKind ] )

  ...
  6. Repeat,
    ...
    g. Else,
      i. Assert: lhsKind is lexical-binding.
      ii. Assert: lhs is a ForDeclaration.
      iii. Let iterationEnv be NewDeclarativeEnvironment(oldEnv).
      iv. Perform ForDeclarationBindingInstantiation of lhs with argument iterationEnv.
      v. Set the running execution context's LexicalEnvironment to iterationEnv.
      vi. If destructuring is true, then
        1. Let status be Completion(ForDeclarationBindingInitialization of lhs
           with arguments nextValue and iterationEnv).
  ...

  KeyedBindingInitialization : BindingElement : BindingPattern Initializer_opt

  ...
  3. If Initializer is present and v is undefined, then
    a. Let defaultValue be ? Evaluation of Initializer.
  ...
flags: [async]
includes: [compareArray.js]
features: [async-functions, destructuring-binding]
---*/

function run(makeFn) {
  var log = [];
  var L = function (x) { log.push(x); };
  Promise.resolve().then(function () { L('w1'); }).then(function () { L('w2'); }).then(function () { L('w3'); });
  var p = makeFn(L);
  L('sync-end');
  return p.then(function () { return log; });
}

// The for-in head binds each enumerated key (a string) to the pattern; a
// string has no own or inherited `b` property, so the default always fires.
async function viaVar(L) {
  for (var { b = await 6 } in { x: 1 }) {
    L('b' + b);
  }
}
async function viaLet(L) {
  for (let { b = await 6 } in { x: 1 }) {
    L('b' + b);
  }
}
async function viaConst(L) {
  for (const { b = await 6 } in { x: 1 }) {
    L('b' + b);
  }
}

var expected = ['sync-end', 'w1', 'b6', 'w2', 'w3'];

Promise.all([run(viaVar), run(viaLet), run(viaConst)])
  .then(function (logs) {
    assert.compareArray(logs[0], expected, 'var');
    assert.compareArray(logs[1], expected, 'let');
    assert.compareArray(logs[2], expected, 'const');
  })
  .then($DONE, $DONE);
