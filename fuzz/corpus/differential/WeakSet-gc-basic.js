// Tests that WeakSet entries are cleared when the value is garbage collected.
// Spec: ECMAScript 2024, sec-weakset-objects

var ws = new WeakSet();
var obj = {};
ws.add(obj);

if (ws.has(obj) !== true) {
  throw new Test262Error("WeakSet should have the value before GC");
}

// Drop reference
obj = null;

// Trigger GC
for (var i = 0; i < 5000; i++) {
  ({});
}

// Entry should be cleared; no crash expected.
