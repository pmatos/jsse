/*---
description: >
  A `yield` inside the default (Initializer) of a for-in/for-of head
  declaration's destructuring pattern suspends the (synchronous) generator
  instead of silently running to completion, both when the loop is the only
  construct in the function (native-statement path) and when another `yield`
  forces it through the compiled state-machine's dedicated loop lowering.
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind, lhsKind,
  labelSet [ , iteratorKind ] )

  ...
  6. Repeat,
    a. Let nextResult be ? Call(iteratorRecord.[[NextMethod]], iteratorRecord.[[Iterator]]).
    ...
    d. If done is true, return V.
    e. Let nextValue be ? IteratorValue(nextResult).
    f. If lhsKind is either assignment or var-binding, then
      i. If destructuring is true, then
        ...
        2. Else,
          a. Assert: lhsKind is var-binding.
          b. Assert: lhs is a ForBinding.
          c. Let status be Completion(BindingInitialization of lhs with
             arguments nextValue and undefined).
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

  Nothing in BindingInitialization / ForDeclarationBindingInitialization
  restricts a `yield` expression from appearing in a SingleNameBinding's
  Initializer, so a generator must suspend at it like any other `yield`; the
  loop body (stmt, step 6.i) must not evaluate until that binding
  initialization completes.
features: [generators, destructuring-binding]
---*/

function* soleForIn() {
  for (let { a = yield 1, b = 2 } in { x: 1 }) {
    return [a, b];
  }
}
var it1 = soleForIn();
var r1 = it1.next();
assert.sameValue(r1.value, 1, 'sole for-in: head pattern default yield suspends');
assert.sameValue(r1.done, false, 'sole for-in: has not completed after first yield');
var r2 = it1.next(5);
assert.sameValue(r2.value[0], 5, 'sole for-in: default-bearing name resumes with sent value');
assert.sameValue(r2.value[1], 2, 'sole for-in: sibling default is still applied after resume');
assert.sameValue(r2.done, true, 'sole for-in: resumes to completion');

function* soleForOf() {
  for (const { a = yield 1, b } of [{ b: 9 }]) {
    return [a, b];
  }
}
var it2 = soleForOf();
it2.next();
var r3 = it2.next(6);
assert.sameValue(r3.value[0], 6, 'sole for-of: default-bearing name resumes with sent value');
assert.sameValue(r3.value[1], 9, 'sole for-of: sibling non-default property is preserved');

function* forcedForOf() {
  yield 0;
  for (const { a = yield 1, b } of [{ b: 9 }]) {
    yield 'tick';
    return [a, b];
  }
}
var it3 = forcedForOf();
assert.sameValue(it3.next().value, 0, 'state-machine for-of: leading yield');
var r4 = it3.next();
assert.sameValue(r4.value, 1, 'state-machine for-of: head pattern default yield suspends');
assert.sameValue(r4.done, false, 'state-machine for-of: has not completed after head yield');
var r5 = it3.next(6);
assert.sameValue(r5.value, 'tick', 'state-machine for-of: resumes into the loop body');
var r6 = it3.next();
assert.sameValue(r6.value[0], 6, 'state-machine for-of: default-bearing name resumes with sent value');
assert.sameValue(r6.value[1], 9, 'state-machine for-of: sibling non-default property is preserved');

function* closureOverForOfHead() {
  yield 0;
  for (const { a = yield 1 } of [{}]) {
    yield 'tick';
    return (() => a)();
  }
}
var it4 = closureOverForOfHead();
it4.next();
it4.next();
assert.sameValue(
  it4.next(42).value,
  'tick',
  'state-machine for-of: yield inside loop body after head suspension resumes body'
);
assert.sameValue(
  it4.next().value,
  42,
  'a closure created in the loop body sees the resumed head-pattern binding'
);

function* varHeadKeepsFunctionScope() {
  yield 0;
  for (var { a = yield 1 } of [{}]) {
    yield 'tick';
  }
  return a;
}
var it5v = varHeadKeepsFunctionScope();
it5v.next();
it5v.next();
it5v.next(9);
assert.sameValue(
  it5v.next().value,
  9,
  'a var head-pattern binding is not desugared into a let, and stays visible after the loop'
);

function* constHeadStillRejectsReassignment() {
  yield 0;
  for (const { a = yield 1 } of [{}]) {
    a = 5;
  }
}
var it5c = constHeadStillRejectsReassignment();
it5c.next();
it5c.next();
assert.throws(
  TypeError,
  function () {
    it5c.next();
  },
  'a const head-pattern binding still rejects reassignment after the desugar'
);

var nextCalls = 0;
function makeCountingIterable(n) {
  return {
    [Symbol.iterator]() {
      var i = 0;
      return {
        next() {
          nextCalls++;
          return i++ < n ? { value: {}, done: false } : { value: undefined, done: true };
        },
      };
    },
  };
}
function* countedLoop(iter) {
  for (const { a = yield 1 } of iter) {
    yield 'tick';
  }
}
var it5 = countedLoop(makeCountingIterable(2));
it5.next();
it5.next('s0');
it5.next();
it5.next('s1');
var last = it5.next();
assert.sameValue(last.done, true, 'counted loop completes after both elements');
assert.sameValue(
  nextCalls,
  3,
  'the underlying iterator is stepped exactly once per element plus one done check, ' +
    'never re-fetched or re-stepped by resuming a head-pattern yield'
);
