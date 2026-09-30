/*---
description: >
  `break`ing out of a `for-of` loop whose head's destructuring default
  contains an `await`, on the iteration after that default has already
  suspended once, still closes the source iterator exactly once -- the
  strip-to-temp rewrite must not leave the driver's normal loop-control
  unwind (`for_of_stack`) unable to find/close the iterator, and must not
  double-close it either. A rejecting default on a later iteration must
  likewise close the iterator exactly once.
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
info: |
  ForIn/OfBodyEvaluation, abrupt completion handling:

  ...
  6. Repeat,
    ...
    j. Let result be Completion(Evaluation of stmt).
    ...
    l. If LoopContinues(result, labelSet) is false, then
      i. Let status be Completion(UpdateEmpty(result, undefined)).
      ii. If iterationKind is enumerate, then
        1. Return ? status.
      iii. Else,
        1. Assert: iterationKind is iterate.
        2. Set status to Completion(IteratorClose(iteratorRecord, status)).
        3. Return ? status.
flags: [async]
includes: [compareArray.js]
features: [async-functions, destructuring-binding]
---*/

function makeIterable(n) {
  var closeCalls = 0;
  var i = 0;
  return {
    closeCalls: function () { return closeCalls; },
    [Symbol.iterator]: function () {
      return {
        next: function () {
          return i < n ? { value: {}, done: false } : { value: undefined, done: true };
        },
        return: function (v) {
          closeCalls++;
          return { done: true, value: v };
        },
      };
    },
  };
}

async function viaBreak() {
  var iterable = makeIterable(5);
  var seen = [];
  for (let { a = await seen.length } of iterable) {
    seen.push(a);
    if (seen.length === 2) {
      break;
    }
  }
  return { seen: seen, closeCalls: iterable.closeCalls() };
}

async function viaReturn() {
  var iterable = makeIterable(5);
  var seen = [];
  await (async function run() {
    for (let { a = await seen.length } of iterable) {
      seen.push(a);
      if (seen.length === 2) {
        return;
      }
    }
  })();
  return { seen: seen, closeCalls: iterable.closeCalls() };
}

async function viaReject() {
  var iterable = makeIterable(5);
  var seen = [];
  var message = null;
  try {
    for (let { a = await (seen.length === 1 ? Promise.reject(new Error('boom')) : seen.length) } of iterable) {
      seen.push(a);
    }
  } catch (e) {
    message = e.message;
  }
  return { seen: seen, closeCalls: iterable.closeCalls(), message: message };
}

Promise.all([viaBreak(), viaReturn(), viaReject()]).then(function (results) {
  assert.compareArray(results[0].seen, [0, 1], 'break: default resumes with per-iteration seen.length');
  assert.sameValue(results[0].closeCalls, 1, 'break: source iterator closed exactly once');

  assert.compareArray(results[1].seen, [0, 1], 'return: default resumes with per-iteration seen.length');
  assert.sameValue(results[1].closeCalls, 1, 'return: source iterator closed exactly once');

  assert.sameValue(results[2].message, 'boom', 'reject: rejecting default propagates its own error');
  assert.compareArray(results[2].seen, [0], 'reject: only the first iteration completed before the rejection');
  assert.sameValue(results[2].closeCalls, 1, 'reject: source iterator closed exactly once');
}).then($DONE, $DONE);
