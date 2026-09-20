/*---
description: >
  In an async function or async generator whose `switch` contains an `await`
  or `yield` in some clause, a clause whose body has no suspension point and
  ends in `break` or `continue` must not fall through into the next clause.
esid: sec-runtime-semantics-caseblockevaluation
info: |
  Runtime Semantics: CaseBlockEvaluation

  Once a clause is selected, the following clauses are evaluated in order
  only until an abrupt completion: "If R is an abrupt completion, return
  ? UpdateEmpty(R, V)". A `break` or `continue` completion is abrupt, so it
  ends the CaseBlock regardless of suspension points in other clauses.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-functions, async-iteration]
---*/

async function asyncFn(x) {
  var log = [];
  switch (x) {
    case 1: log.push("one"); break;
    case 2: log.push("two"); break;
    case 3: log.push("three"); await 0; break;
    default: log.push("def");
  }
  return log.join(",");
}

async function* asyncGen(x) {
  var log = [];
  switch (x) {
    case 1: log.push("one"); break;
    case 2: log.push("two"); break;
    case 3: log.push("three"); yield 0; break;
    default: log.push("def");
  }
  yield log.join(",");
}

async function collect(gen) {
  var out = [];
  for await (var v of gen) {
    out.push(v);
  }
  return out;
}

async function* continueGen() {
  var log = [];
  for (var i = 0; i < 4; i++) {
    switch (i) {
      case 0: log.push("zero"); continue;
      case 1: log.push("one"); continue;
      case 2: yield "two"; break;
      default: log.push("d" + i);
    }
    log.push("after" + i);
  }
  yield log.join(",");
}

async function continueFn() {
  var log = [];
  for (var i = 0; i < 4; i++) {
    switch (i) {
      case 0: log.push("zero"); continue;
      case 1: log.push("one"); continue;
      case 2: await 0; break;
      default: log.push("d" + i);
    }
    log.push("after" + i);
  }
  return log.join(",");
}

async function forAwaitBody() {
  var log = [];
  for await (var v of [1, 2, 3, 4]) {
    switch (v) {
      case 1: log.push("one"); break;
      case 2: log.push("two"); continue;
      case 3: await 0; log.push("three"); break;
      default: log.push("def");
    }
    log.push("tail" + v);
  }
  return log.join(",");
}

asyncTest(async function() {
  assert.sameValue(await asyncFn(1), "one", "async function, case 1");
  assert.sameValue(await asyncFn(2), "two", "async function, case 2");
  assert.sameValue(await asyncFn(3), "three", "async function, awaiting case");
  assert.sameValue(await asyncFn(4), "def", "async function, default");

  assert.compareArray(await collect(asyncGen(1)), ["one"], "async generator, case 1");
  assert.compareArray(await collect(asyncGen(2)), ["two"], "async generator, case 2");
  assert.compareArray(await collect(asyncGen(3)), [0, "three"], "async generator, yielding case");
  assert.compareArray(await collect(asyncGen(4)), ["def"], "async generator, default");

  assert.compareArray(
    await collect(continueGen()),
    ["two", "zero,one,after2,d3,after3"],
    "async generator, continue from a yield-free clause"
  );
  assert.sameValue(
    await continueFn(),
    "zero,one,after2,d3,after3",
    "async function, continue from a yield-free clause"
  );
  assert.sameValue(
    await forAwaitBody(),
    "one,tail1,two,three,tail3,def,tail4",
    "for-await body, break and continue from clauses"
  );
});
