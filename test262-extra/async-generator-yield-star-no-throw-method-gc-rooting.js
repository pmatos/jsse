// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asynciteratorclose
description: >
  While AsyncIteratorClose waits on a `yield*` delegate's `return()` call
  (no `throw` method, `.throw()` in flight), the pending "no throw method"
  TypeError must stay reachable across a garbage collection triggered from
  inside `return()` itself, even though nothing else references it yet --
  it has not been thrown or stored anywhere at that point.
info: |
  AsyncIteratorClose ( iteratorRecord, completion )

  4.c. If IsCallable(return) is true, then
    i. Set innerResult to Completion(Call(return, iterator)).
  8. Return ? completion.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration, host-gc-required]
---*/

asyncTest(async function () {
  var delegate = {
    [Symbol.asyncIterator]() { return this; },
    next() { return Promise.resolve({ value: 1, done: false }); },
    return(v) {
      $262.gc();
      return { value: v, done: true };
    },
  };
  var it = (async function* () {
    try {
      yield* delegate;
    } catch (e) {
      return e;
    }
  })();

  await it.next();
  var result = await it.throw(new Error('injected'));

  assert.sameValue(result.done, true, 'the body completed normally after catching');
  assert.sameValue(
    result.value.constructor,
    TypeError,
    'the "no throw method" TypeError survives the gc() inside return(), not a corrupted/reused object'
  );
  assert.sameValue(
    result.value.message,
    'The iterator does not provide a throw method',
    'the TypeError carries its original message, not a stale/reused value'
  );
});
