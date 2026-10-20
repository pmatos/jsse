/*---
description: Call-site dispatch preserves receivers, argument order, completions and live operands
esid: sec-evaluatecall
info: |
  EvaluateCall evaluates arguments before checking callability and calls the
  function with the reference's receiver. Call-site caching must preserve
  these steps across repeated calls, reentry, and changes of callable.
includes: [compareArray.js]
---*/

function invoke(holder, value) {
  return holder.method(value);
}

function first(value) {
  "use strict";
  assert.sameValue(this, holder);
  return value + 1;
}
function second(value) {
  "use strict";
  assert.sameValue(this, holder);
  return value + 10;
}
var holder = { method: first };
for (var i = 0; i < 5; i++) assert.sameValue(invoke(holder, i), i + 1);
first.marker = 1;
assert.sameValue(invoke(holder, 5), 6);
holder.method = second;
assert.sameValue(invoke(holder, 5), 15);

var traps = 0;
holder.method = new Proxy(first, {
  apply: function(target, receiver, args) {
    traps++;
    assert.sameValue(receiver, holder);
    return target.apply(receiver, args);
  }
});
for (i = 0; i < 5; i++) assert.sameValue(invoke(holder, i), i + 1);
assert.sameValue(traps, 5);
holder.method = first.bind(holder, 20);
assert.sameValue(invoke(holder, 100), 21);

var error = {};
holder.method = function() { throw error; };
for (i = 0; i < 3; i++) {
  try {
    invoke(holder, i);
    throw new Test262Error("call must throw");
  } catch (caught) {
    assert.sameValue(caught, error);
  }
}
holder.method = class {};
assert.throws(TypeError, function() { invoke(holder, 0); });
var revocable = Proxy.revocable(first, {});
holder.method = revocable.proxy;
revocable.revoke();
assert.throws(TypeError, function() { invoke(holder, 0); });

var events = [];
var noncallable = {
  get method() { events.push("callee"); return 0; }
};
function argument() { events.push("argument"); return 1; }
assert.throws(TypeError, function() { noncallable.method(argument()); });
assert.compareArray(events, ["callee", "argument"]);

// Reentry observes different functions at invoke's same call site.
holder.method = function(value) {
  holder.method = second;
  assert.sameValue(invoke(holder, value), value + 10);
  holder.method = first;
  assert.sameValue(invoke(holder, value), value + 1);
  return value + 100;
};
assert.sameValue(invoke(holder, 7), 107);
assert.sameValue(invoke(holder, 7), 8);

// Fresh receiver and arguments exist only in the caller's operand roots.
function makeHolder() {
  return {
    marker: 23,
    method: function(a, b) {
      $262.gc();
      return this.marker + a.value + b.value;
    }
  };
}
function collectingCall() {
  return makeHolder().method({ value: 10 }, { value: 9 });
}
for (i = 0; i < 5; i++) assert.sameValue(collectingCall(), 42);

function directEval() {
  var local = 31;
  return eval("local + 11");
}
assert.sameValue(directEval(), 42);
function tail(fn, value) {
  "use strict";
  return fn(value);
}
assert.sameValue(tail(function(x) { return x + 1; }, 41), 42);
