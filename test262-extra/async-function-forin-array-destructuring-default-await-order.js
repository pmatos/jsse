/*---
description: >
  An `await` inside a destructuring default in a `for-in` head's *array*
  Variable-kind binding suspends the async function at a real Await state --
  the caller continues synchronously and jobs already queued run before the
  function's continuation -- exactly like `await` in an object for-in-head
  pattern default (issue #773), or a plain declaration's default (issue
  #725). `ForOfHead` is the shared driver terminator for both `for-in` and
  `for-of` in this engine.
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

  Runtime Semantics: IteratorBindingInitialization
  ArrayBindingPattern : [ Elision_opt BindingRestElement_opt ]
  SingleNameBinding : BindingIdentifier Initializer_opt

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

// The for-in head binds each enumerated key (a one-character string, "x")
// to the pattern; `[a, b = await 6]` destructures the string's own
// (single) UTF-16 code unit into `a`, then finds the string exhausted for
// `b`, so the default always fires.
async function viaVar(L) {
  for (var [a, b = await 6] in { x: 1 }) {
    L('a' + a + 'b' + b);
  }
}
async function viaLet(L) {
  for (let [a, b = await 6] in { x: 1 }) {
    L('a' + a + 'b' + b);
  }
}
async function viaConst(L) {
  for (const [a, b = await 6] in { x: 1 }) {
    L('a' + a + 'b' + b);
  }
}

var expected = ['sync-end', 'w1', 'axb6', 'w2', 'w3'];

Promise.all([run(viaVar), run(viaLet), run(viaConst)])
  .then(function (logs) {
    assert.compareArray(logs[0], expected, 'var');
    assert.compareArray(logs[1], expected, 'let');
    assert.compareArray(logs[2], expected, 'const');
  })
  .then($DONE, $DONE);
