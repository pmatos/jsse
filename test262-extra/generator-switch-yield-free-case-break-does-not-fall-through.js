/*---
description: >
  In a generator whose `switch` contains a `yield` in some clause, a clause
  whose body has no `yield` and ends in `break` must not fall through into the
  next clause. The `break` completion ends CaseBlockEvaluation.
esid: sec-runtime-semantics-caseblockevaluation
info: |
  Runtime Semantics: CaseBlockEvaluation

  Once a clause is selected, the following clauses are evaluated in order
  only until an abrupt completion: "If R is an abrupt completion, return
  ? UpdateEmpty(R, V)". A `break` completion (sec-break-statement-runtime-
  semantics-evaluation) therefore stops fall-through, whether or not another
  clause of the same `switch` contains a suspension point.
includes: [compareArray.js]
features: [generators]
---*/

function* plain(x) {
  var log = [];
  switch (x) {
    case 1: log.push("one"); break;
    case 2: log.push("two"); break;
    case 3: log.push("three"); yield 0; break;
    default: log.push("def");
  }
  yield log.join(",");
}

function run(gen) {
  return Array.from(gen);
}

assert.compareArray(run(plain(1)), ["one"], "case 1 stops at break");
assert.compareArray(run(plain(2)), ["two"], "case 2 stops at break");
assert.compareArray(run(plain(3)), [0, "three"], "yielding case 3");
assert.compareArray(run(plain(4)), ["def"], "default");

function* blockWrapped(x) {
  var log = [];
  switch (x) {
    case 1: { log.push("a"); break; }
    case 2: { log.push("b"); break; }
    case 3: yield "y"; break;
    default: log.push("d");
  }
  yield log.join(",");
}

assert.compareArray(run(blockWrapped(1)), ["a"], "block-wrapped break, case 1");
assert.compareArray(run(blockWrapped(2)), ["b"], "block-wrapped break, case 2");
assert.compareArray(run(blockWrapped(3)), ["y", ""], "yielding case");
assert.compareArray(run(blockWrapped(9)), ["d"], "default");

function* conditionalBreak(x, c) {
  var log = [];
  switch (x) {
    case 1:
      log.push("a");
      if (c) break;
      log.push("b");
    case 2:
      log.push("c");
      break;
    case 3:
      yield "y";
  }
  yield log.join(",");
}

assert.compareArray(run(conditionalBreak(1, true)), ["a"], "if-break taken");
assert.compareArray(run(conditionalBreak(1, false)), ["a,b,c"], "if-break not taken falls through");
assert.compareArray(run(conditionalBreak(2, true)), ["c"], "case 2");
assert.compareArray(run(conditionalBreak(3, true)), ["y", ""], "yielding case");

function* middleDefault(x) {
  var log = [];
  switch (x) {
    case 1: log.push("one"); break;
    default: log.push("def"); break;
    case 2: log.push("two"); yield "t"; break;
    case 3: log.push("three");
  }
  yield log.join(",");
}

assert.compareArray(run(middleDefault(1)), ["one"], "case before default");
assert.compareArray(run(middleDefault(7)), ["def"], "default in the middle stops at break");
assert.compareArray(run(middleDefault(2)), ["t", "two"], "case after default");
assert.compareArray(run(middleDefault(3)), ["three"], "last case");

function* lastCaseBreak(x) {
  var log = [];
  switch (x) {
    case 1: yield "y"; break;
    case 2: log.push("two"); break;
  }
  yield log.join(",");
}

assert.compareArray(run(lastCaseBreak(2)), ["two"], "last case with break");
assert.compareArray(run(lastCaseBreak(1)), ["y", ""], "yielding case");

function* fallThroughStillWorks(x) {
  var log = [];
  switch (x) {
    case 1: log.push(1);
    case 2: log.push(2); break;
    case 3: yield 0; break;
  }
  yield log.join(",");
}

assert.compareArray(run(fallThroughStillWorks(1)), ["1,2"], "yield-free fall-through is preserved");
assert.compareArray(run(fallThroughStillWorks(2)), ["2"], "case 2");
assert.compareArray(run(fallThroughStillWorks(3)), [0, ""], "yielding case");
