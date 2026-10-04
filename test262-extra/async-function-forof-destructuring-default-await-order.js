/*---
description: >
  An `await` inside a destructuring default in a `for-of` head's
  Variable-kind binding suspends the async function at a real Await state --
  the caller continues synchronously and jobs already queued run before the
  function's continuation -- exactly like an `await` in a plain
  declaration's default.
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

  Await ( value )

  ...
  9. Perform PerformPromiseThen(promise, onFulfilled, onRejected).
  10. Remove asyncContext from the execution context stack ...
flags: [async]
includes: [compareArray.js]
features: [async-functions, async-iteration, destructuring-binding]
---*/

function run(makeFn) {
  var log = [];
  var L = function (x) { log.push(x); };
  Promise.resolve().then(function () { L('w1'); }).then(function () { L('w2'); }).then(function () { L('w3'); });
  var p = makeFn(L);
  L('sync-end');
  return p.then(function () { return log; });
}

async function viaVar(L) {
  for (var { b = await 6 } of [{}]) {
    L('b' + b);
  }
}
async function viaLet(L) {
  for (let { b = await 6 } of [{}]) {
    L('b' + b);
  }
}
async function viaConst(L) {
  for (const { b = await 6 } of [{}]) {
    L('b' + b);
  }
}
async function viaAsyncGenerator(L) {
  async function* g() {
    for (var { b = await 6 } of [{}]) {
      L('b' + b);
    }
  }
  var r = g().next();
  L('called-next');
  await r;
}

var expected = ['sync-end', 'w1', 'b6', 'w2', 'w3'];

Promise.all([run(viaVar), run(viaLet), run(viaConst), run(viaAsyncGenerator)])
  .then(function (logs) {
    assert.compareArray(logs[0], expected, 'var');
    assert.compareArray(logs[1], expected, 'let');
    assert.compareArray(logs[2], expected, 'const');
    assert.compareArray(
      logs[3],
      ['called-next', 'sync-end', 'w1', 'b6', 'w2', 'w3'],
      'async generator'
    );
  })
  .then($DONE, $DONE);
