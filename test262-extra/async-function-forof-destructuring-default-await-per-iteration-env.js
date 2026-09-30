/*---
description: >
  A lexical (`let`) `for-of` head whose destructuring default contains an
  `await` still gets a fresh binding per iteration -- a closure created in
  one iteration keeps that iteration's own binding, not a shared mutable
  slot aliased by every closure.
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
flags: [async]
features: [async-functions, destructuring-binding]
---*/

async function f() {
  var fns = [];
  var i = 0;
  for (let { a = await i } of [{}, {}]) {
    i++;
    fns.push(function () { return a; });
  }
  return fns;
}

f().then(function (fns) {
  assert.sameValue(fns[0](), 0, 'closure from the first iteration keeps its own binding');
  assert.sameValue(fns[1](), 1, 'closure from the second iteration keeps its own binding');
}).then($DONE, $DONE);
