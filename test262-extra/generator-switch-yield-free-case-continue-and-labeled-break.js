/*---
description: >
  In a generator whose `switch` contains a `yield` in some clause, a
  yield-free clause that ends in `continue`, `continue label`, or `break label`
  transfers control to the loop or label it targets, and does not run the
  statements of the following clauses.
esid: sec-continue-statement-runtime-semantics-evaluation
info: |
  ContinueStatement : continue ; returns Completion { [[Type]]: continue,
  [[Value]]: empty, [[Target]]: empty }, and BreakStatement : break LabelIdentifier ;
  returns a break completion with that label as its target. Both are abrupt
  completions, so CaseBlockEvaluation stops evaluating clauses
  (sec-runtime-semantics-caseblockevaluation) and the completion propagates to
  the enclosing iteration statement or LabelledStatement
  (sec-runtime-semantics-labelledevaluation).
includes: [compareArray.js]
features: [generators]
---*/

function run(gen) {
  return Array.from(gen);
}

function* continueInFor() {
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

assert.compareArray(
  run(continueInFor()),
  ["two", "zero,one,after2,d3,after3"],
  "continue in for"
);

function* continueInWhile() {
  var log = [];
  var i = 0;
  while (i < 3) {
    var n = i++;
    switch (n) {
      case 0: log.push("zero"); continue;
      case 1: yield "one"; break;
      default: log.push("d" + n);
    }
    log.push("after" + n);
  }
  yield log.join(",");
}

assert.compareArray(
  run(continueInWhile()),
  ["one", "zero,after1,d2,after2"],
  "continue in while"
);

function* continueInDoWhile() {
  var log = [];
  var i = 0;
  do {
    var n = i++;
    switch (n) {
      case 0: log.push("zero"); continue;
      case 1: yield "one"; break;
      default: log.push("d" + n);
    }
    log.push("after" + n);
  } while (i < 3);
  yield log.join(",");
}

assert.compareArray(
  run(continueInDoWhile()),
  ["one", "zero,after1,d2,after2"],
  "continue in do-while"
);

function* continueInForOf() {
  var log = [];
  for (var v of ["a", "b", "c"]) {
    switch (v) {
      case "a": log.push("A"); continue;
      case "b": yield "B"; break;
      default: log.push("D");
    }
    log.push("after" + v);
  }
  yield log.join(",");
}

assert.compareArray(
  run(continueInForOf()),
  ["B", "A,afterb,D,afterc"],
  "continue in for-of"
);

function* continueOuter() {
  var log = [];
  outer: for (var i = 0; i < 3; i++) {
    for (var j = 0; j < 3; j++) {
      switch (j) {
        case 0: log.push("i" + i + "j0"); continue;
        case 1: log.push("i" + i + "j1"); continue outer;
        case 2: yield "unreachable"; break;
      }
      log.push("unreachable-after");
    }
  }
  yield log.join(",");
}

assert.compareArray(
  run(continueOuter()),
  ["i0j0,i0j1,i1j0,i1j1,i2j0,i2j1"],
  "continue outer from a yield-free clause"
);

function* breakOuterLoop() {
  var log = [];
  outer: for (var i = 0; i < 5; i++) {
    switch (i) {
      case 0: log.push("zero"); break;
      case 1: log.push("one"); break outer;
      case 2: yield "two"; break;
    }
    log.push("after" + i);
  }
  yield log.join(",");
}

assert.compareArray(
  run(breakOuterLoop()),
  ["zero,after0,one"],
  "break outer from a yield-free clause leaves the loop"
);

function* breakLabeledBlock(x) {
  var log = [];
  block: {
    switch (x) {
      case 1: log.push("one"); break block;
      case 2: yield "two"; break;
    }
    log.push("after-switch");
  }
  log.push("after-block");
  yield log.join(",");
}

assert.compareArray(run(breakLabeledBlock(1)), ["one,after-block"], "break to labeled block");
assert.compareArray(run(breakLabeledBlock(2)), ["two", "after-switch,after-block"], "yielding clause");
assert.compareArray(run(breakLabeledBlock(3)), ["after-switch,after-block"], "no clause");

function* nestedSwitch(x, y) {
  var log = [];
  switch (x) {
    case 1:
      switch (y) {
        case 1: log.push("inner1"); break;
        case 2: log.push("inner2"); break;
      }
      log.push("outer1-tail");
      break;
    case 2:
      log.push("outer2");
      break;
    case 3:
      yield "y";
  }
  yield log.join(",");
}

assert.compareArray(
  run(nestedSwitch(1, 1)),
  ["inner1,outer1-tail"],
  "inner break does not leave the outer clause early or fall through"
);
assert.compareArray(run(nestedSwitch(1, 2)), ["inner2,outer1-tail"], "inner clause 2");
assert.compareArray(run(nestedSwitch(2, 0)), ["outer2"], "outer clause 2");
assert.compareArray(run(nestedSwitch(3, 0)), ["y", ""], "yielding clause");

function* returnAndThrow(x) {
  switch (x) {
    case 1: return "returned";
    case 2: throw new Test262Error("thrown");
    case 3: yield "y"; break;
  }
  yield "end";
}

assert.compareArray(run(returnAndThrow(1)), [], "return from a yield-free clause");
assert.throws(Test262Error, function() {
  run(returnAndThrow(2));
}, "throw from a yield-free clause");
assert.compareArray(run(returnAndThrow(3)), ["y", "end"], "yielding clause");
assert.compareArray(run(returnAndThrow(4)), ["end"], "no clause");
