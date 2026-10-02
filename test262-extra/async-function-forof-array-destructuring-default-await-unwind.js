/*---
description: >
  `break`ing or `return`ing out of a `for-of` loop whose head's *array*
  destructuring default contains an `await`, on the iteration after that
  default has already suspended once, still closes the source (outer)
  iterator exactly once -- mirroring the object-pattern case (issue #773).
  A rejecting default closes both the source iterator *and* the per-iteration
  array pattern's own iterator ($dstr_iter) exactly once -- the array-specific
  risk the object-pattern test can't cover, since an object pattern never
  opens an iterator of its own. The per-iteration array iterator here is
  left deliberately not-yet-exhausted (a third element after the rejecting
  default) so IteratorClose has real work to do, not a no-op on an
  already-Done record.
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

  Runtime Semantics: IteratorBindingInitialization
  ArrayBindingPattern : [ BindingElementList , Elision_opt BindingRestElement_opt ]

  ...
  If an abrupt completion propagates out of binding an element while
  iteratorRecord.[[Done]] is false, IteratorClose is performed on that
  element's own iteratorRecord.
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
          return i < n ? { value: [1], done: false } : { value: undefined, done: true };
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
  for (let [a, b = await seen.length] of iterable) {
    seen.push(a + ':' + b);
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
    for (let [a, b = await seen.length] of iterable) {
      seen.push(a + ':' + b);
      if (seen.length === 2) {
        return;
      }
    }
  })();
  return { seen: seen, closeCalls: iterable.closeCalls() };
}

// `[a, b = await ...]`'s own per-iteration iterator (over `[1, undefined,
// 'never-reached']`) is still open -- not yet Done -- when `b`'s default
// rejects: `a` consumed the first element, `b`'s default fired on the
// second (`undefined`, not absent -- still a real, non-Done step), leaving
// a third element unconsumed. This is the shape that needs an explicit
// IteratorClose on the array pattern's own iterator, not just the outer one.
function makeInnerIterable() {
  var closeCalls = 0;
  var values = [1, undefined, 'never-reached'];
  var i = 0;
  return {
    closeCalls: function () { return closeCalls; },
    [Symbol.iterator]: function () {
      return {
        next: function () {
          return i < values.length ? { value: values[i++], done: false } : { value: undefined, done: true };
        },
        return: function (v) {
          closeCalls++;
          return { done: true, value: v };
        },
      };
    },
  };
}

// Yields `value` once, then is exhausted -- like `makeIterable`, but able to
// carry an arbitrary per-iteration value (`makeIterable` always yields
// `[1]`), since `viaReject` needs the outer for-of's own stepped value to
// *be* the instrumented inner iterable.
function makeOuterIterable(value) {
  var closeCalls = 0;
  var i = 0;
  return {
    closeCalls: function () { return closeCalls; },
    [Symbol.iterator]: function () {
      return {
        next: function () {
          return i++ < 1 ? { value: value, done: false } : { value: undefined, done: true };
        },
        return: function (v) {
          closeCalls++;
          return { done: true, value: v };
        },
      };
    },
  };
}

async function viaReject() {
  var innerIterable = makeInnerIterable();
  var outerIterable = makeOuterIterable(innerIterable);
  var message = null;
  try {
    for (let [a, b = await Promise.reject(new Error('boom'))] of outerIterable) {
    }
  } catch (e) {
    message = e.message;
  }
  return {
    message: message,
    innerCloses: innerIterable.closeCalls(),
    outerCloses: outerIterable.closeCalls(),
  };
}

Promise.all([viaBreak(), viaReturn(), viaReject()]).then(function (results) {
  assert.compareArray(results[0].seen, ['1:0', '1:1'], 'break: default resumes with per-iteration seen.length');
  assert.sameValue(results[0].closeCalls, 1, 'break: outer source iterator closed exactly once');

  assert.compareArray(results[1].seen, ['1:0', '1:1'], 'return: default resumes with per-iteration seen.length');
  assert.sameValue(results[1].closeCalls, 1, 'return: outer source iterator closed exactly once');

  assert.sameValue(results[2].message, 'boom', 'reject: rejecting default propagates its own error');
  assert.sameValue(
    results[2].innerCloses,
    1,
    'reject: the array pattern\'s own still-open iterator is closed exactly once, not leaked or double-closed'
  );
  assert.sameValue(
    results[2].outerCloses,
    1,
    'reject: the outer for-of source iterator is also closed exactly once'
  );
}).then($DONE, $DONE);
