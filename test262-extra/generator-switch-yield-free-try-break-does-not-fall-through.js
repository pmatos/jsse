/*---
description: >
  In a generator whose `switch` contains a `yield` in some clause, a clause
  whose body has no `yield` but whose `break` sits inside a `try` must run the
  finalizer and then leave the CaseBlock instead of falling through.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  Runtime Semantics: CaseBlockEvaluation

  Once a clause is selected, the following clauses are evaluated in order
  only until an abrupt completion: "If R is an abrupt completion, return
  ? UpdateEmpty(R, V)". A `break` completion ends the CaseBlock.

  TryStatement : try Block Finally

  Let B be Completion(Evaluation of Block). Let F be Completion(Evaluation of
  Finally). If F is a normal completion, set F to B. So the finalizer runs and
  the `break` completion is what the TryStatement returns.
includes: [compareArray.js]
features: [generators]
---*/

function run(gen) {
  return Array.from(gen);
}

function* tryFinally(x) {
  var l = [];
  switch (x) {
    case 1: try { break; } finally { l.push("f"); }
    case 2: l.push("two"); break;
    case 3: yield 0;
  }
  yield l.join();
}

assert.compareArray(run(tryFinally(1)), ["f"], "finalizer runs, break does not fall through");
assert.compareArray(run(tryFinally(2)), ["two"], "case 2 is unaffected");
assert.compareArray(run(tryFinally(3)), [0, ""], "yielding case");

function* tryCatch(x) {
  var l = [];
  switch (x) {
    case 1: try { throw 0; } catch (e) { l.push("c"); break; }
    case 2: l.push("two"); break;
    case 3: yield 0;
  }
  yield l.join();
}

assert.compareArray(run(tryCatch(1)), ["c"], "break in catch clause");

function* finallyBreak(x) {
  var l = [];
  switch (x) {
    case 1: try { l.push("t"); } finally { l.push("f"); break; }
    case 2: l.push("two"); break;
    case 3: yield 0;
  }
  yield l.join();
}

assert.compareArray(run(finallyBreak(1)), ["t,f"], "break in finally block");

function* nestedTry(x) {
  var l = [];
  switch (x) {
    case 1:
      try {
        try { break; } finally { l.push("inner"); }
      } finally { l.push("outer"); }
    case 2: l.push("two"); break;
    case 3: yield 0;
  }
  yield l.join();
}

assert.compareArray(run(nestedTry(1)), ["inner,outer"], "nested try runs both finalizers");

function* finallyOverrides(x) {
  var l = [];
  outer: for (var i = 0; i < 3; i++) {
    switch (x) {
      case 1:
        try { break; } finally { l.push("f" + i); continue outer; }
      case 2: l.push("two"); break;
      case 3: yield 0;
    }
    l.push("after" + i);
  }
  yield l.join();
}

assert.compareArray(
  run(finallyOverrides(1)),
  ["f0,f1,f2"],
  "a finalizer's continue replaces the try block's break"
);

function* finallyReturns(x) {
  var l = [];
  switch (x) {
    case 1: try { break; } finally { l.push("f"); return l.join(); }
    case 2: l.push("two"); break;
    case 3: yield 0;
  }
  yield "unreachable";
}

assert.compareArray(run(finallyReturns(1)), [], "a finalizer's return replaces the break");

function* nonJumpingTry(x) {
  var l = [];
  switch (x) {
    case 1: try { l.push("t"); } finally { l.push("f"); }
    case 2: l.push("two"); break;
    case 3: yield 0;
  }
  yield l.join();
}

assert.compareArray(
  run(nonJumpingTry(1)),
  ["t,f,two"],
  "a try that completes normally still falls through"
);

function* tryContinue() {
  var l = [];
  for (var i = 0; i < 3; i++) {
    switch (i) {
      case 0: try { continue; } finally { l.push("f" + i); }
      case 1: l.push("one"); break;
      case 2: yield "y";
    }
    l.push("after" + i);
  }
  yield l.join();
}

assert.compareArray(
  run(tryContinue()),
  ["y", "f0,one,after1,after2"],
  "continue from a switch clause's try reaches the loop"
);
