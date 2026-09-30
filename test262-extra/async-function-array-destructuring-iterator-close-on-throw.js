/*---
description: >
  An abrupt completion that crosses a still-open array-pattern iterator --
  a later element's awaited default rejecting after an earlier element has
  already stepped, or the pattern's own default promise rejecting -- closes
  the iterator exactly once, via the same for-of-stack unwind path a real
  for-of loop's abrupt exit already uses (issue #725).
esid: sec-runtime-semantics-iteratorbindinginitialization
info: |
  IteratorBindingInitialization

  SingleNameBinding : BindingIdentifier Initializer_opt
  ...
  4. If Initializer is present and v is undefined, then
    a. Let defaultValue be ? Evaluation of Initializer.
    b. Set v to ? GetValue(defaultValue).
flags: [async]
features: [async-functions, Symbol.iterator, destructuring-binding]
---*/

// Never signals [[Done]], so both elements below always reach their default.
function makeIterable(onReturn) {
  var it = {};
  it[Symbol.iterator] = function () {
    return {
      next: function () { return { value: undefined, done: false }; },
      return: function () { onReturn(); return {}; },
    };
  };
  return it;
}

async function rejectingDefault(it) {
  var [a = await Promise.reject(new Test262Error('rejected default'))] = it;
  return a;
}

async function laterElementRejects(it) {
  var [a = 1, b = await Promise.reject(new Test262Error('rejected default'))] = it;
  return a + ',' + b;
}

Promise.resolve().then(function () {
  var closeCount = 0;
  var it = makeIterable(function () { closeCount++; });
  return rejectingDefault(it).then(
    function () { throw new Test262Error('expected rejection'); },
    function (e) {
      assert.sameValue(e.message, 'rejected default');
      assert.sameValue(closeCount, 1, 'return() is called exactly once');
    }
  );
}).then(function () {
  var closeCount = 0;
  var it = makeIterable(function () { closeCount++; });
  return laterElementRejects(it).then(
    function () { throw new Test262Error('expected rejection'); },
    function (e) {
      assert.sameValue(e.message, 'rejected default');
      assert.sameValue(closeCount, 1, 'return() is called exactly once');
    }
  );
}).then($DONE, $DONE);
