/*---
description: >
  Calls of one async function, generator or async generator keep separate
  bindings, completion state and resumption points when several calls of the
  same function literal are suspended at once, when calls recurse, and when
  the function value comes from different closures of one literal.
esid: sec-async-functions-abstract-operations-async-function-start
info: |
  Each call creates a fresh execution context and environment
  (OrdinaryCallEvaluateBody, AsyncFunctionStart, GeneratorStart), so the code
  of a function body may be shared between calls, but no per-call state may
  leak from one call into another suspended call of the same body.
includes: [compareArray.js]
flags: [async]
features: [async-iteration]
---*/

var releases = [];
function gate(i) {
  return new Promise(function (resolve) { releases[i] = resolve; });
}

async function worker(id, n) {
  var log = [];
  let total = 0;
  for (let i = 0; i < n; i++) {
    total += await gate(id * 10 + i);
    log.push(id + ":" + i);
  }
  try {
    if (id === 2) throw new Error("boom" + id);
    return { id: id, total: total, log: log.join(",") };
  } catch (e) {
    return { id: id, total: -total, log: e.message };
  }
}

async function countdown(n) {
  if (n === 0) return "done";
  const inner = await countdown(n - 1);
  return n + "," + inner;
}

function* walk(tree) {
  if (tree === null) return;
  yield* walk(tree.left);
  yield tree.value;
  yield* walk(tree.right);
}

function makeCounter(start) {
  return function* () {
    let k = start;
    while (true) yield k++;
  };
}

function makeTicker(label) {
  return async function* () {
    for (let i = 0; i < 2; i++) {
      await null;
      yield label + i;
    }
  };
}

var tree = {
  value: 4,
  left: { value: 2, left: { value: 1, left: null, right: null }, right: { value: 3, left: null, right: null } },
  right: { value: 5, left: null, right: null }
};
assert.compareArray(Array.from(walk(tree)), [1, 2, 3, 4, 5], "recursive generator delegation");

var counterA = makeCounter(10)();
var counterB = makeCounter(100)();
assert.sameValue(counterA.next().value, 10, "first closure, first value");
assert.sameValue(counterB.next().value, 100, "second closure, first value");
assert.sameValue(counterA.next().value, 11, "first closure resumes its own state");
assert.sameValue(counterB.next().value, 101, "second closure resumes its own state");

var p1 = worker(1, 2);
var p2 = worker(2, 2);
var p3 = worker(3, 1);

async function collectTickers() {
  var a = makeTicker("a")();
  var b = makeTicker("b")();
  var out = [];
  out.push((await a.next()).value, (await b.next()).value);
  out.push((await b.next()).value, (await a.next()).value);
  out.push((await a.next()).done, (await b.next()).done);
  return out;
}

Promise.all([p1, p2, p3, countdown(4), collectTickers()])
  .then(function (results) {
    assert.sameValue(results[0].total, 1 + 2, "worker 1 total");
    assert.sameValue(results[0].log, "1:0,1:1", "worker 1 log");
    assert.sameValue(results[1].total, -(20 + 30), "worker 2 total from its catch block");
    assert.sameValue(results[1].log, "boom2", "worker 2 caught its own error");
    assert.sameValue(results[2].total, 7, "worker 3 total");
    assert.sameValue(results[2].log, "3:0", "worker 3 log");
    assert.sameValue(results[3], "4,3,2,1,done", "recursive async calls");
    assert.compareArray(results[4], ["a0", "b0", "b1", "a1", true, true], "interleaved async generators");
  })
  .then($DONE, $DONE);

// Resume the suspended workers out of call order. A gate exists only once its
// worker has reached that await, so wait for it to appear.
function release(i, v) {
  if (releases[i]) {
    releases[i](v);
    return Promise.resolve();
  }
  return Promise.resolve().then(function () { return release(i, v); });
}
release(30, 7)
  .then(function () { return release(20, 20); })
  .then(function () { return release(10, 1); })
  .then(function () { return release(21, 30); })
  .then(function () { return release(11, 2); });
