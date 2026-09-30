/*---
description: >
  Normal-completion IteratorClose runs when a lowered (suspension-
  containing) array binding pattern doesn't fully drain its iterator --
  matching the sync-only test262 pair ary-init-iter-close.js /
  ary-init-iter-no-close.js, now for the state-machine path an awaiting
  default takes (issue #725). `.return()` is *not* called when the
  pattern ends in a rest element (rest always drains to [[Done]]) or when
  it fully drains the iterator's own values.
esid: sec-runtime-semantics-bindinginitialization
info: |
  BindingPattern : ArrayBindingPattern

  ...
  3. Let result be Completion(IteratorBindingInitialization for
     ArrayBindingPattern using iteratorRecord and environment).
  4. If iteratorRecord.[[Done]] is false, return ? IteratorClose(iteratorRecord, result).
  5. Return result.
flags: [async]
features: [async-functions, Symbol.iterator, destructuring-binding]
---*/

function makeIterable(values, onReturn) {
  var it = {};
  it[Symbol.iterator] = function () {
    var i = 0;
    return {
      next: function () {
        return { value: values[i], done: i++ >= values.length };
      },
      return: function () {
        onReturn();
        return {};
      },
    };
  };
  return it;
}

// `done: true` is signalled on the very step that returns the final value,
// matching the sync-only reference test's ary-init-iter-no-close.js shape --
// the pattern's own last Step already observes [[Done]], so Finish is a
// no-op rather than performing an (invalid, per sec-iteratorclose) close on
// an already-done iterator.
function makeExactlyExhaustingIterable(value, onReturn) {
  var it = {};
  it[Symbol.iterator] = function () {
    var called = false;
    return {
      next: function () {
        if (called) return { value: undefined, done: true };
        called = true;
        return { value: value, done: true };
      },
      return: function () {
        onReturn();
        return {};
      },
    };
  };
  return it;
}

async function underConsumed(it) {
  var [a = await 1] = it;
  return a;
}

async function endsInRest(it) {
  var [a = await 1, ...rest] = it;
  return rest;
}

async function fullyDrained(it) {
  var [a = await 1] = it;
  return a;
}

Promise.resolve().then(function () {
  var closeCount = 0;
  var it = makeIterable([1, 2, 3], function () { closeCount++; });
  return underConsumed(it).then(function () {
    assert.sameValue(closeCount, 1, 'return() is called when the pattern under-consumes the iterator');
  });
}).then(function () {
  var closeCount = 0;
  var it = makeIterable([1, 2, 3], function () { closeCount++; });
  return endsInRest(it).then(function () {
    assert.sameValue(closeCount, 0, 'return() is not called when the pattern ends in a rest element');
  });
}).then(function () {
  var closeCount = 0;
  var it = makeExactlyExhaustingIterable(1, function () { closeCount++; });
  return fullyDrained(it).then(function () {
    assert.sameValue(closeCount, 0, 'return() is not called when the pattern fully drains the iterator');
  });
}).then($DONE, $DONE);
