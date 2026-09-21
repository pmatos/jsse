/*---
description: >
  Two symptoms from issue #684's report do not reproduce on this engine and
  have no production fix in this change: a `for-in` loop whose body contains
  an `await` still runs its body every iteration (the lowering does not skip
  it), and a `var` declared outside a `for (let ...)` loop does not collide
  with the loop head's lexical binding (they are different bindings in
  different scopes). Locked in here so the `for`-head per-iteration-binding
  work in this change doesn't accidentally reintroduce either.
esid: sec-for-in-and-for-of-statements-runtime-semantics-labelledevaluation
info: |
  ForIn/OfBodyEvaluation evaluates the loop body once per enumerated
  property, regardless of whether the body suspends.

  sec-for-statement's early error ("It is a Syntax Error if any element of
  the BoundNames of LexicalDeclaration also occurs in the VarDeclaredNames of
  Statement") is about names bound by the *loop body*, not about an
  unrelated `var` declared outside the loop entirely.
flags: [async]
includes: [compareArray.js]
features: [async-functions]
---*/

async function forInBodyRunsEveryIteration() {
  let seen = [];
  let obj = { a: 1, b: 2, c: 3 };
  for (const key in obj) {
    await 0;
    seen.push(key);
  }
  return seen;
}

async function varOutsideForLetHeadIsLegal() {
  var i = 'outer-var';
  let seen = [];
  for (let i = 0; i < 2; i++) {
    await 0;
    seen.push(i);
  }
  return { seen: seen, outerVar: i };
}

forInBodyRunsEveryIteration()
  .then(function (seen) {
    assert.compareArray(seen, ['a', 'b', 'c'], 'a for-in body with an await still runs every iteration');
    return varOutsideForLetHeadIsLegal();
  })
  .then(function (result) {
    assert.compareArray(result.seen, [0, 1], 'the for-head let binding iterates normally');
    assert.sameValue(result.outerVar, 'outer-var', 'an unrelated outer var is untouched by the loop head');
  })
  .then($DONE, $DONE);
