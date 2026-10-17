// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-evaluatecall
description: >
  A call site's callee cache belongs to its function literal: closures of one
  literal share it, a site that sees different callees (functions, a Proxy, a
  bound function, a class constructor, a non-callable) behaves like a cold
  site every time, and recursion through a site keeps working.
features: [Proxy, class]
flags: [noStrict]
---*/

function makeCaller() {
  return function (f, x) {
    return f(x);
  };
}
var first = makeCaller();
var second = makeCaller();

function plus1(x) {
  return x + 1;
}
function plus2(x) {
  return x + 2;
}
var trapped = 0;
var proxied = new Proxy(plus1, {
  apply: function (target, thisArg, args) {
    trapped++;
    return target.apply(thisArg, args) * 100;
  }
});
var bound = plus2.bind(null);
class Klass {}

var callees = [plus1, plus2, proxied, bound, plus1, plus1, proxied];
var expected = [2, 3, 200, 3, 2, 2, 200];
for (var round = 0; round < 3; round++) {
  callees.forEach(function (f, i) {
    var caller = i % 2 ? first : second;
    assert.sameValue(caller(f, 1), expected[i], "round " + round + " callee " + i);
  });
  assert.throws(TypeError, function () {
    first(Klass, 1);
  }, "class constructor at a warmed-up site, round " + round);
  assert.throws(TypeError, function () {
    second({}, 1);
  }, "non-callable at a warmed-up site, round " + round);
  assert.sameValue(first(plus1, 1), 2, "site recovers after a throw, round " + round);
}
assert.sameValue(trapped, 6, "the Proxy trap ran for every Proxy callee");

function countdown(n) {
  return n === 0 ? 0 : 1 + countdown(n - 1);
}
for (var i = 0; i < 4; i++) {
  assert.sameValue(countdown(50), 50, "recursion through one call site, round " + i);
}

var swapped = function (x) {
  return x;
};
function site() {
  return swapped(5);
}
assert.sameValue(site(), 5);
assert.sameValue(site(), 5);
swapped = function (x) {
  return x * 2;
};
assert.sameValue(site(), 10, "the same site calls a different function object");
swapped = plus1;
Object.defineProperty(plus1, "extra", { value: 1 });
assert.sameValue(site(), 6, "shape change on the cached callee");
