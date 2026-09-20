/*---
description: >
  In an async generator whose `switch` or loop contains a `yield` (or `await`),
  a yield-free `try` statement whose `break` or `continue` leaves the enclosing
  clause or loop must run its finalizer and then perform the jump.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  TryStatement : try Block Finally

  If F is a normal completion, set F to B: the `break`/`continue` completion
  of the block is the completion of the TryStatement, and it ends
  CaseBlockEvaluation (sec-runtime-semantics-caseblockevaluation) or the
  iteration (sec-loopcontinues).
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

async function collect(gen) {
  var out = [];
  for await (var v of gen) {
    out.push(v);
  }
  return out;
}

async function* switchTry(x) {
  var l = [];
  switch (x) {
    case 1: try { break; } finally { l.push("f"); }
    case 2: l.push("two"); break;
    case 3: yield 0;
  }
  yield l.join();
}

async function* switchTryAwait(x) {
  var l = [];
  switch (x) {
    case 1: try { break; } finally { l.push("f"); }
    case 2: l.push("two"); break;
    case 3: await 0;
  }
  yield l.join();
}

async function* forBreak() {
  var l = [];
  for (var i = 0; i < 5; i++) {
    try { if (i == 2) break; } finally { l.push("f" + i); }
    yield i;
  }
  yield l.join();
}

async function* forContinue() {
  var l = [];
  for (var i = 0; i < 4; i++) {
    try { if (i == 1) continue; } finally { l.push("f" + i); }
    yield i;
  }
  yield l.join();
}

async function* labeledContinue() {
  var l = [];
  outer: for (var i = 0; i < 3; i++) {
    for (var j = 0; j < 3; j++) {
      try { if (j == 1) continue outer; } finally { l.push(i + "" + j); }
      yield i * 10 + j;
    }
  }
  yield l.join();
}

asyncTest(async function() {
  assert.compareArray(await collect(switchTry(1)), ["f"], "yielding switch, try + break");
  assert.compareArray(await collect(switchTry(2)), ["two"], "case 2");
  assert.compareArray(await collect(switchTry(3)), [0, ""], "yielding case");
  assert.compareArray(await collect(switchTryAwait(1)), ["f"], "awaiting switch, try + break");
  assert.compareArray(await collect(forBreak()), [0, 1, "f0,f1,f2"], "for + break");
  assert.compareArray(await collect(forContinue()), [0, 2, 3, "f0,f1,f2,f3"], "for + continue");
  assert.compareArray(
    await collect(labeledContinue()),
    [0, 10, 20, "00,01,10,11,20,21"],
    "continue to a labeled outer loop"
  );
});
