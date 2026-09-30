/*---
description: >
  IteratorStepValue sets [[Done]] to true before propagating a failure from
  either IteratorStep (`next()` throws) or IteratorValue (the result's
  `.value` getter throws), so a subsequent element's own Step never sees a
  live iterator to close, and no enclosing IteratorClose calls `.return()`
  either -- matching the sync-only reference pair
  ary-ptrn-elem-id-iter-step-err.js / ary-ptrn-elem-id-iter-val-err.js, now
  for the state-machine path a following awaiting element forces (issue
  #725).
esid: sec-iteratorstepvalue
info: |
  IteratorStepValue ( iteratorRecord )

  1. Let result be Completion(IteratorStep(iteratorRecord)).
  2. If result is a throw completion, set iteratorRecord.[[Done]] to true.
  3. ReturnIfAbrupt(result).
  ...
  6. Let value be Completion(IteratorValue(result)).
  7. If value is a throw completion, set iteratorRecord.[[Done]] to true.
  8. ReturnIfAbrupt(value).
flags: [async]
features: [async-functions, Symbol.iterator, destructuring-binding]
---*/

function run(makeIterable) {
  var closeCount = 0;
  var it = makeIterable(function () { closeCount++; });

  async function f() {
    var [a = await 1, b = await 2] = it;
    return a + ',' + b;
  }

  return f().then(
    function () { throw new Test262Error('expected the promise to reject'); },
    function (e) {
      assert.sameValue(e.message, 'boom');
      assert.sameValue(closeCount, 0, 'return() must not be called once [[Done]] is already true');
    }
  );
}

var stepThrows = function (onReturn) {
  var it = {};
  it[Symbol.iterator] = function () {
    return {
      next: function () { throw new Test262Error('boom'); },
      return: onReturn,
    };
  };
  return it;
};

var valueGetterThrows = function (onReturn) {
  var it = {};
  it[Symbol.iterator] = function () {
    return {
      next: function () {
        return {
          done: false,
          get value() { throw new Test262Error('boom'); },
        };
      },
      return: onReturn,
    };
  };
  return it;
};

Promise.all([
  run(stepThrows),
  run(valueGetterThrows),
]).then(function () {}).then($DONE, $DONE);
