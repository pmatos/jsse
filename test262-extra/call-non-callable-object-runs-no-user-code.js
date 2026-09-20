/*---
description: >
  Calling a non-callable object throws a TypeError without running user code:
  building the error must not read the callee's `constructor` or `toString`.
esid: sec-evaluatecall
info: |
  13.3.6.2 EvaluateCall ( func, ref, arguments, tailPosition )
    4. If IsCallable(func) is false, throw a TypeError exception.
---*/

var getterRan = false;
var callee = {};
Object.defineProperty(callee, "constructor", {
  get: function() { getterRan = true; return Object; },
});
Object.defineProperty(callee, "toString", {
  get: function() { getterRan = true; return function() { return "x"; }; },
});

assert.throws(TypeError, function() {
  callee();
});
assert.sameValue(getterRan, false, "the callee's accessors must not run");

assert.throws(TypeError, function() {
  new callee();
});
assert.sameValue(getterRan, false, "the callee's accessors must not run for new");
