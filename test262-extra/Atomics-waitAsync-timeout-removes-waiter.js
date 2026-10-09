/*---
description: >
  A waitAsync waiter whose timeout expires is removed from the waiter list, so a
  later Atomics.notify does not count it.
esid: sec-atomics.waitasync
info: |
  When the timeout of an asynchronous waiter fires, the Waiter Record is removed
  from the WaiterList and the promise resolves with "timed-out".
flags: [async]
features: [Atomics, Atomics.waitAsync, SharedArrayBuffer, TypedArray]
---*/

var ta = new Int32Array(new SharedArrayBuffer(8));
var result = Atomics.waitAsync(ta, 0, 0, 20);
assert.sameValue(result.async, true);
assert.sameValue(Atomics.notify(ta, 1), 0, "waiter is keyed by index");

result.value.then(function(v) {
  assert.sameValue(v, "timed-out");
  assert.sameValue(Atomics.notify(ta, 0), 0, "timed-out waiter is no longer in the list");
}).then($DONE, $DONE);
