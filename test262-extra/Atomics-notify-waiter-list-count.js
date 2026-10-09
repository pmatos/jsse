/*---
description: >
  Atomics.notify wakes waitAsync waiters, honours the count
  argument, and waiters registered through different views of the same
  SharedArrayBuffer location share one waiter list.
esid: sec-atomics.notify
info: |
  Atomics.notify removes up to count waiters from the WaiterList for the
  (block, byte index) pair and returns the number removed. Notifying a different location wakes nobody.
includes: [compareArray.js]
flags: [async]
features: [Atomics, Atomics.waitAsync, SharedArrayBuffer, TypedArray]
---*/

var sab = new SharedArrayBuffer(16);
var whole = new Int32Array(sab);
var shifted = new Int32Array(sab, 4);
var order = [];

function track(name, result) {
  assert.sameValue(result.async, true, name + " waits asynchronously");
  return result.value.then(function(v) { order.push(name + ":" + v); });
}

var p1 = track("a", Atomics.waitAsync(whole, 1, 0, 10000));
var p2 = track("b", Atomics.waitAsync(shifted, 0, 0, 10000));
var p3 = track("c", Atomics.waitAsync(whole, 1, 0, 10000));

assert.sameValue(Atomics.notify(whole, 0), 0, "different location has no waiters");
assert.sameValue(Atomics.notify(whole, 1, 0), 0, "count 0 wakes nobody");
assert.sameValue(Atomics.notify(whole, 1, -1), 0, "negative count wakes nobody");
assert.sameValue(Atomics.notify(whole, 1, 1), 1, "count 1 wakes exactly one waiter");
assert.sameValue(Atomics.notify(whole, 1), 2, "undefined count wakes the remaining waiters");
assert.sameValue(Atomics.notify(whole, 1), 0, "waiter list is empty afterwards");

Promise.all([p1, p2, p3]).then(function() {
  assert.compareArray(order.slice().sort(), ["a:ok", "b:ok", "c:ok"], "all waiters resolved ok");
}).then($DONE, $DONE);
