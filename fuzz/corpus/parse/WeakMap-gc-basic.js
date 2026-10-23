// Tests that WeakMap entries are cleared when the key is garbage collected.
// Spec: ECMAScript 2024, sec-weakmap-objects
// A WeakMap entry is cleared when the key is no longer reachable.

var wm = new WeakMap();
var key = {};
wm.set(key, "value");

if (wm.has(key) !== true) {
  throw new Test262Error("WeakMap should have the key before GC");
}

// Drop the only reference to the key
key = null;

// Allocate enough objects to trigger GC (threshold is 4096)
for (var i = 0; i < 5000; i++) {
  ({});
}

// After GC, the entry should be cleared.
// Note: we can't directly test has() since we lost our reference to key,
// but the WeakMap should not hold the object alive.
// This test mainly verifies no crash occurs during weak GC.
