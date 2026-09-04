// Tests that WeakMap values are retrievable while the key is still reachable.
// Spec: ECMAScript 2024, sec-weakmap-objects

var wm = new WeakMap();
var key = {};
var value = { data: 42 };
wm.set(key, value);

// Drop direct reference to value, but key is still reachable
value = null;

// Trigger GC
for (var i = 0; i < 5000; i++) {
  ({});
}

// Key is still reachable, so the entry (and its value) must survive
if (wm.has(key) !== true) {
  throw new Test262Error("WeakMap entry should survive when key is reachable");
}

var retrieved = wm.get(key);
if (retrieved === undefined || retrieved.data !== 42) {
  throw new Test262Error("WeakMap value should be retrievable: got " + (retrieved && retrieved.data));
}
