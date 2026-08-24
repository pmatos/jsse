/*---
description: >
  A pending Atomics.waitAsync promise keeps its resolving functions reachable
  after its creating call frame returns and across a forced collection.
esid: sec-atomics.waitasync
info: |
  DoWait creates a PromiseCapability and stores it in the asynchronous Waiter
  Record. EnqueueResolveInAgentJob later calls that capability's resolve
  function when the timeout expires or the waiter is notified.

  Regression test for issue #465: JSSE kept the resolve function on an
  evaluator-frame temporary-root stack, so a collection after waitAsync
  returned could reclaim it and leave the returned promise pending forever.
flags: [async]
features: [Atomics.waitAsync, SharedArrayBuffer, TypedArray, Atomics, host-gc-required]
---*/

function startPendingWait() {
  var array = new Int32Array(new SharedArrayBuffer(Int32Array.BYTES_PER_ELEMENT));
  var result = Atomics.waitAsync(array, 0, 0, 10);
  assert.sameValue(result.async, true, "the wait is asynchronous");
  return result.value;
}

var waitPromise = startPendingWait();

waitPromise
  .then(function (result) {
    assert.sameValue(result, "timed-out");
  })
  .then($DONE, $DONE);

$262.gc();
