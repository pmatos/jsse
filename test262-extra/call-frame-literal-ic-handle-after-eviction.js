/*---
description: A hot function keeps working after enough distinct bodies have run to evict its inline-cache store
info: |
  10.2.1.3 Runtime Semantics: EvaluateBody

  Evaluating a function body is observable only through its result. Running
  more distinct function bodies than the engine retains per-body caches for
  must not change what an earlier function, called again afterwards, returns.
esid: sec-ordinary-callable-call
flags: [noStrict]
---*/

function hot(o, n) {
  var total = 0;
  for (var i = 0; i < n; i++) {
    total += o.x + o.y;
  }
  return total;
}

var shapeA = { x: 1, y: 2 };
var shapeB = { y: 20, x: 10, z: 0 };
assert.sameValue(hot(shapeA, 4), 12);
assert.sameValue(hot(shapeB, 4), 120);

function viaCaller() {
  return hot(shapeA, 2) + hot(shapeB, 1);
}
assert.sameValue(viaCaller(), 6 + 30);

var churn = 9000;
for (var i = 0; i < churn; i++) {
  var f = new Function("o", "return o.x + " + i + ";");
  if (f({ x: 1 }) !== 1 + i) {
    throw new Test262Error("churn function " + i + " returned a wrong value");
  }
  if (i % 3000 === 0) {
    assert.sameValue(hot(shapeA, 3), 9, "hot function while bodies churn");
  }
}

assert.sameValue(hot(shapeA, 4), 12, "after eviction, shape A");
assert.sameValue(hot(shapeB, 4), 120, "after eviction, shape B");
assert.sameValue(viaCaller(), 6 + 30, "after eviction, nested call");

function recursiveHot(n) {
  return n === 0 ? 0 : 1 + recursiveHot(n - 1);
}
assert.sameValue(recursiveHot(50), 50);
for (var i = 0; i < churn; i++) {
  new Function("return " + i + ";")();
}
assert.sameValue(recursiveHot(50), 50, "recursion across an eviction");
