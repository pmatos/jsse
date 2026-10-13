/*---
description: >
  The bindings of a `for (let/const ...;;)` head are in their temporal dead
  zone while the head's initializers run, whether the declarator is a plain
  identifier or a destructuring pattern, so a read of the name never resolves
  to an outer binding of the same name.
info: |
  ForLoopEvaluation (14.7.4.2) creates loopEnv and instantiates the head's
  bound names in it (step 5.d-e) before evaluating the LexicalDeclaration
  (step 6), so an initializer that reads one of them throws a ReferenceError.
esid: sec-for-statement-runtime-semantics-labelledevaluation
---*/

function identifierHead() {
  let i = 5;
  for (let i = i; ;) {
    return i;
  }
}
assert.throws(ReferenceError, identifierHead, "let identifier head");

function patternHead() {
  let i = 5;
  for (let [i] = [i]; ;) {
    return i;
  }
}
assert.throws(ReferenceError, patternHead, "let array pattern head");

function constHead() {
  const k = 9;
  for (const k = k + 1; ;) {
    return k;
  }
}
assert.throws(ReferenceError, constHead, "const identifier head");

function laterDeclaratorSeesEarlier() {
  for (let a = 1, b = a + 1; ;) {
    return b;
  }
}
assert.sameValue(laterDeclaratorSeesEarlier(), 2, "earlier declarator is initialised");

function laterDeclaratorIsInTdz() {
  for (let a = b, b = 1; ;) {
    return a;
  }
}
assert.throws(ReferenceError, laterDeclaratorIsInTdz, "later declarator is in TDZ");
