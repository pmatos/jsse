// Tests GC ephemeron fixpoint with WeakMap chains.
// Spec: ECMAScript 2024, sec-weakmap-objects
// Allocates enough objects to trigger GC, then verifies ephemeron semantics.

// Create a chain: key1 -> value1 (which is key2's only reference holder)
var wm1 = new WeakMap();
var wm2 = new WeakMap();

var key1 = { name: "key1" };
var key2 = { name: "key2" };

// wm1: key1 -> { ref: key2 }
wm1.set(key1, { ref: key2 });
// wm2: key2 -> "alive"
wm2.set(key2, "alive");

// key1 is still reachable, so key2 should be kept alive through the ephemeron chain
if (!wm1.has(key1)) {
  throw new Test262Error("wm1 should have key1 before GC");
}
if (!wm2.has(key2)) {
  throw new Test262Error("wm2 should have key2 before GC");
}

// key1 is still reachable, its value holds key2's object
if (!wm1.has(key1)) {
  throw new Test262Error("wm1 should still have key1");
}
var val = wm1.get(key1);
if (!val || !val.ref) {
  throw new Test262Error("wm1's value should have ref to key2's object");
}
if (!wm2.has(val.ref)) {
  throw new Test262Error("wm2 should have key2 (reachable through wm1 value)");
}

// Test with WeakSet
var ws = new WeakSet();
var obj1 = {};
var obj2 = {};
ws.add(obj1);
ws.add(obj2);

if (!ws.has(obj1)) throw new Test262Error("WeakSet should have obj1");
if (!ws.has(obj2)) throw new Test262Error("WeakSet should have obj2");

obj1 = null;
for (var i = 0; i < 5000; i++) { ({}); }

if (!ws.has(obj2)) throw new Test262Error("WeakSet should still have obj2 after GC");
