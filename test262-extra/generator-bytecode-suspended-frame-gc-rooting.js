/*---
description: >
  The operand stack, pending reference, block environments and variables of a
  suspended generator body are kept alive by the generator alone, across
  minor and major collections, and survive being resumed afterwards.
esid: sec-generator-function-definitions-runtime-semantics-evaluation
features: [generators]
---*/

function churn() {
  var junk = [];
  for (var i = 0; i < 200; i++) junk.push({ i: i, s: "x" + i, a: [i] });
  return junk.length;
}

function collect() {
  churn();
  if (typeof $262 !== "undefined" && $262.gc) $262.gc();
  churn();
}

// Objects evaluated before a yield live only on the operand stack.
function* operands() {
  var result = [{ tag: "first" }, [1, 2, 3], new Map([["k", "v"]]), yield "pause", { tag: "last" }];
  return result;
}
var it = operands();
it.next();
collect();
var result = it.next("sent").value;
assert.sameValue(result[0].tag, "first");
assert.sameValue(result[1].join(), "1,2,3");
assert.sameValue(result[2].get("k"), "v");
assert.sameValue(result[3], "sent");
assert.sameValue(result[4].tag, "last");

// A call's callee, receiver and arguments are held across the yield.
function* callOperands() {
  var obj = { name: "recv", m(a, b, c) { return this.name + a.x + b.y + c; } };
  return obj.m({ x: 1 }, { y: 2 }, yield "pause");
}
it = callOperands();
it.next();
collect();
assert.sameValue(it.next(3).value, "recv123");

// A pending assignment target is held across the yield.
function* assignTarget() {
  var holder = {};
  (function() { return holder; })().slot = yield "pause";
  return holder.slot;
}
it = assignTarget();
it.next();
collect();
assert.sameValue(it.next({ kept: true }).value.kept, true);

// Block environments and their closures survive collections between resumes.
function* environments() {
  for (let i = 0; i < 3; i++) {
    const big = { i: i, payload: [i, i + 1] };
    const read = () => big.payload.join();
    yield 0;
    collect();
    yield read();
  }
}
it = environments();
var got = [];
for (var step of it) got.push(step);
assert.sameValue(got.join(), "0,0,1,0,1,2,0,2,3");

// A for-of iterator on the operand stack is reachable only from the frame.
function* iterating() {
  for (const item of (function*() { for (var n = 0; n < 4; n++) yield { n: n }; })()) {
    collect();
    yield item.n;
  }
}
assert.sameValue(Array.from(iterating()).join(), "0,1,2,3");

// A try/finally record on the stack across collections.
function* finallyRecord() {
  try { yield "body"; return { v: "ret" }; }
  finally { collect(); yield "in-finally"; collect(); }
}
it = finallyRecord();
assert.sameValue(it.next().value, "body");
assert.sameValue(it.next().value, "in-finally");
var last = it.next();
assert.sameValue(last.value.v, "ret");
assert.sameValue(last.done, true);

// return() through a finally that collects keeps the pending value.
it = finallyRecord();
it.next();
assert.sameValue(it.return({ v: "injected" }).value, "in-finally");
assert.sameValue(it.next().value.v, "injected");
