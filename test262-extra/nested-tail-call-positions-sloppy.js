/*---
description: Nested tail-position calls in sloppy code are ordinary calls with the same observable results
esid: sec-evaluatecall
info: |
  Outside strict mode a call in tail position is not required to reuse the
  caller's frame, but the completion values, evaluation order, this values and
  cleanup behaviour of `return` with a conditional, logical or comma operand
  are unchanged. A call returned out of a for-of body is not in tail position,
  so the iterator is closed before the function completes.
flags: [noStrict]
---*/

var calls = 0;
function leaf() { calls++; return "leaf"; }

function f1(a) { return a && leaf(); }
function f2(a) { return a || leaf(); }
function f3(a) { return a ?? leaf(); }
function f4(a) { return a ? leaf() : "no"; }
function f5(a) { return a, leaf(); }
assert.sameValue(f1(0), 0, "&& short-circuits");
assert.sameValue(f1(1), "leaf", "&& calls");
assert.sameValue(f2("x"), "x", "|| short-circuits");
assert.sameValue(f2(0), "leaf", "|| calls");
assert.sameValue(f3(0), 0, "?? short-circuits on a non-nullish value");
assert.sameValue(f3(null), "leaf", "?? calls");
assert.sameValue(f4(false), "no", "?: alternate");
assert.sameValue(f4(true), "leaf", "?: consequent");
assert.sameValue(f5(1), "leaf", "comma tail");
assert.sameValue(calls, 5, "the leaf ran once per non-short-circuited path");

var obj = {
  tag: "obj",
  who: function () { return this === undefined ? "undefined" : this === globalThis ? "global" : this.tag; },
  viaAnd: function (c) { return c && this.who(); },
  viaIndirect: function () { return 0, (0, this.who)(); }
};
assert.sameValue(obj.viaAnd(true), "obj", "member callee keeps its base");
assert.sameValue(obj.viaIndirect(), "global", "indirect callee gets the global object as this in sloppy code");

function thrower() { throw new Error("boom"); }
function catcher(c) {
  try {
    return c ? thrower() : "none";
  } catch (e) {
    return "caught:" + e.message;
  }
}
assert.sameValue(catcher(true), "caught:boom", "a throw from the leaf reaches the try");
assert.sameValue(catcher(false), "none", "the other branch returns normally");

var closed = 0;
var iterable = {
  [Symbol.iterator]: function () {
    return {
      next: function () { return { value: 1, done: false }; },
      return: function () { closed++; return {}; }
    };
  }
};
function inLoop(c) {
  for (var x of iterable) {
    return c ? leaf() : x;
  }
}
assert.sameValue(inLoop(true), "leaf", "the call result is returned out of for-of");
assert.sameValue(closed, 1, "the iterator is closed when a ?: call leaf returns out of for-of");
assert.sameValue(inLoop(false), 1, "a non-call branch returns out of for-of");
assert.sameValue(closed, 2, "the iterator is closed for the non-call branch too");
function inLoopAnd(c) {
  for (var x of iterable) {
    return c && leaf();
  }
}
assert.sameValue(inLoopAnd(true), "leaf", "the && call result is returned out of for-of");
assert.sameValue(closed, 3, "the iterator is closed for an && call leaf");

function Ctor(c) { return c ? leaf() : 0; }
assert.sameValue(Object.getPrototypeOf(new Ctor(true)), Ctor.prototype, "a constructor returning a non-object yields this");
