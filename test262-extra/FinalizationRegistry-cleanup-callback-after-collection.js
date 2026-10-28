/*---
description: >
  A FinalizationRegistry calls its cleanup callback with the held value, once,
  in a later job, after the registered target is collected; unregister removes
  a cell so its callback never runs, even when the target is already dead.
esid: sec-cleanup-finalization-registry
info: |
  CleanupFinalizationRegistry removes each cell whose [[WeakRefTarget]] is
  empty and calls the callback with its [[HeldValue]]. FinalizationRegistry.prototype.unregister
  removes every cell whose [[UnregisterToken]] matches, whether or not the target is empty.
flags: [async]
includes: [asyncHelpers.js]
features: [FinalizationRegistry, host-gc-required]
---*/

var calls = [];
var registry = new FinalizationRegistry(function (held) {
  calls.push(held);
});
var token = {};

(function () {
  registry.register({}, "plain");
  registry.register({}, "unregistered", token);
})();

async function settle() {
  for (var i = 0; i < 20; i++) {
    await null;
  }
}

asyncTest(async function () {
  $262.gc();
  assert.sameValue(registry.unregister(token), true, "dead target's cell is still unregistrable");
  await settle();
  assert.sameValue(calls.length, 1, "only the cell that stayed registered is cleaned up");
  assert.sameValue(calls[0], "plain");

  $262.gc();
  await settle();
  assert.sameValue(calls.length, 1, "a cell is cleaned up once");
  assert.sameValue(registry.unregister(token), false, "cleaned and unregistered cells are gone");
});
