/*---
description: >
  An `await` inside a destructuring default in a `for await` head's *array*
  Variable-kind binding suspends the async function at a real Await state,
  alongside the mandatory per-step `Await(nextResult)` (§14.7.5.7 step 6.b) --
  which still happens exactly once per iteration, in addition to the
  default's own `Await` -- exactly like an object for-await-of-head pattern
  default (issue #773).
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind, lhsKind,
  labelSet [ , iteratorKind ] )

  ...
  6. Repeat,
    a. Let nextResult be ? Call(iteratorRecord.[[NextMethod]], iteratorRecord.[[Iterator]]).
    b. If iteratorKind is async, set nextResult to ? Await(nextResult).
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
includes: [compareArray.js]
features: [async-functions, async-iteration, destructuring-binding]
---*/

function asyncIterableOf(items) {
  var nextCalls = 0;
  return {
    nextCalls: function () { return nextCalls; },
    [Symbol.asyncIterator]: function () {
      var i = 0;
      return {
        next: function () {
          nextCalls++;
          return Promise.resolve(
            i < items.length ? { value: items[i++], done: false } : { value: undefined, done: true }
          );
        },
      };
    },
  };
}

function run(makeFn) {
  var log = [];
  var L = function (x) { log.push(x); };
  Promise.resolve().then(function () { L('w1'); }).then(function () { L('w2'); }).then(function () { L('w3'); });
  var p = makeFn(L);
  L('sync-end');
  return p.then(function (extra) { return { log: log, extra: extra }; });
}

async function viaLet(L) {
  var iterable = asyncIterableOf([[1]]);
  for await (let [a, b = await 1] of iterable) {
    L('a' + a + 'b' + b);
  }
  return iterable.nextCalls();
}

var expected = ['sync-end', 'w1', 'w2', 'a1b1', 'w3'];

run(viaLet).then(function (result) {
  assert.compareArray(
    result.log,
    expected,
    'the head default\'s own await suspends alongside the mandatory per-step Await(nextResult)'
  );
  assert.sameValue(
    result.extra,
    2,
    'next() is called once per element plus once for the done check, never re-stepped'
  );
}).then($DONE, $DONE);
