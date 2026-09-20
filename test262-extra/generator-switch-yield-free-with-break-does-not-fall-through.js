/*---
description: >
  In a generator whose `switch` contains a `yield` in some clause, a clause
  whose yield-free body breaks out of a `with` statement must leave the
  CaseBlock, and the `with` body must still resolve names through its object.
esid: sec-with-statement-runtime-semantics-evaluation
info: |
  WithStatement : with ( Expression ) Statement

  The completion of the body is returned (after the object environment is
  popped): "Return ? UpdateEmpty(C, undefined)". A `break` completion is
  therefore what the WithStatement returns, and it ends CaseBlockEvaluation
  (sec-runtime-semantics-caseblockevaluation).
includes: [compareArray.js]
features: [generators]
flags: [noStrict]
---*/

function run(gen) {
  return Array.from(gen);
}

function* withBreak(x) {
  var k = "outer";
  var o = { k: "inner" };
  var l = [];
  switch (x) {
    case 1: with (o) { l.push(k); break; }
    case 2: l.push("two"); break;
    case 3: yield 0;
  }
  l.push(k);
  yield l.join();
}

assert.compareArray(run(withBreak(1)), ["inner,outer"], "with body sees o.k, break leaves the switch");
assert.compareArray(run(withBreak(2)), ["two,outer"], "case 2");
assert.compareArray(run(withBreak(3)), [0, "outer"], "yielding case");

function* withFallsThrough(x) {
  var o = { k: "inner" };
  var l = [];
  switch (x) {
    case 1: with (o) { l.push(k); }
    case 2: l.push("two"); break;
    case 3: yield 0;
  }
  yield l.join();
}

assert.compareArray(
  run(withFallsThrough(1)),
  ["inner,two"],
  "a with body that completes normally still falls through"
);

function* withTryBreak(x) {
  var o = { k: "inner" };
  var l = [];
  switch (x) {
    case 1:
      with (o) {
        try { l.push(k); break; } finally { l.push("f"); }
      }
    case 2: l.push("two"); break;
    case 3: yield 0;
  }
  yield l.join();
}

assert.compareArray(run(withTryBreak(1)), ["inner,f"], "try nested in with");

function* withContinue() {
  var o = { k: "inner" };
  var l = [];
  for (var i = 0; i < 3; i++) {
    with (o) { l.push(k + i); if (i == 1) continue; }
    yield i;
  }
  yield l.join();
}

assert.compareArray(run(withContinue()), [0, 2, "inner0,inner1,inner2"], "continue out of a with body");
