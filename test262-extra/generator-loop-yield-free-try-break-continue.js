/*---
description: >
  In a generator loop whose body yields, a yield-free `try` statement whose
  `break` or `continue` targets the loop must run its finalizer and then
  perform the jump instead of continuing with the rest of the iteration.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  TryStatement : try Block Finally

  If F is a normal completion, set F to B: the `break`/`continue` completion
  of the block is the completion of the TryStatement.

  LoopContinues (sec-loopcontinues) decides whether a `continue` completion
  is consumed by the loop that contains it.
includes: [compareArray.js]
features: [generators, Symbol.iterator]
---*/

function run(gen) {
  return Array.from(gen);
}

function* forBreak() {
  var l = [];
  for (var i = 0; i < 5; i++) {
    try { if (i == 2) break; } finally { l.push("f" + i); }
    yield i;
  }
  yield l.join();
}

assert.compareArray(run(forBreak()), [0, 1, "f0,f1,f2"], "for + break");

function* forContinue() {
  var l = [];
  for (var i = 0; i < 4; i++) {
    try { if (i == 1) continue; } finally { l.push("f" + i); }
    yield i;
  }
  yield l.join();
}

assert.compareArray(run(forContinue()), [0, 2, 3, "f0,f1,f2,f3"], "for + continue");

function* whileBreak() {
  var l = [];
  var i = 0;
  while (i < 5) {
    try { if (i == 2) break; } finally { l.push("f" + i); }
    yield i++;
  }
  yield l.join();
}

assert.compareArray(run(whileBreak()), [0, 1, "f0,f1,f2"], "while + break");

function* doWhileContinue() {
  var l = [];
  var n = 0;
  do {
    n++;
    try { if (n == 2) continue; } finally { l.push("f" + n); }
    yield n;
  } while (n < 4);
  yield l.join();
}

assert.compareArray(run(doWhileContinue()), [1, 3, 4, "f1,f2,f3,f4"], "do-while + continue");

function* labeledContinue() {
  var l = [];
  outer: for (var i = 0; i < 3; i++) {
    for (var j = 0; j < 3; j++) {
      try { if (j == 1) continue outer; } finally { l.push(i + "" + j); }
      yield i * 10 + j;
    }
  }
  yield l.join();
}

assert.compareArray(
  run(labeledContinue()),
  [0, 10, 20, "00,01,10,11,20,21"],
  "continue to a labeled outer loop"
);

function* labeledBreak() {
  var l = [];
  outer: for (var i = 0; i < 3; i++) {
    for (var j = 0; j < 3; j++) {
      try { if (i == 1 && j == 1) break outer; } finally { l.push(i + "" + j); }
      yield i * 10 + j;
    }
  }
  yield l.join();
}

assert.compareArray(
  run(labeledBreak()),
  [0, 1, 2, 10, "00,01,02,10,11"],
  "break out of a labeled outer loop"
);

function iterable(log) {
  return {
    [Symbol.iterator]() {
      var i = -1;
      return {
        next() {
          i++;
          return { value: i, done: i > 3 };
        },
        return() {
          log.push("return");
          return {};
        }
      };
    }
  };
}

function* forOfBreak(log) {
  for (var v of iterable(log)) {
    try { if (v == 1) break; } finally { log.push("f" + v); }
    yield v;
  }
  yield "end";
}

var breakLog = [];
assert.compareArray(run(forOfBreak(breakLog)), [0, "end"], "for-of + break");
assert.compareArray(breakLog, ["f0", "f1", "return"], "break closes the iterator once");

function* forOfContinue(log) {
  for (var v of iterable(log)) {
    try { if (v == 1) continue; } finally { log.push("f" + v); }
    yield v;
  }
  yield "end";
}

var continueLog = [];
assert.compareArray(run(forOfContinue(continueLog)), [0, 2, 3, "end"], "for-of + continue");
assert.compareArray(
  continueLog,
  ["f0", "f1", "f2", "f3"],
  "continue does not close the iterator"
);

function* jumpInCatch() {
  var l = [];
  for (var i = 0; i < 4; i++) {
    try { throw i; } catch (e) { if (e == 2) break; l.push("c" + e); }
    yield i;
  }
  yield l.join();
}

assert.compareArray(run(jumpInCatch()), [0, 1, "c0,c1"], "break from a catch clause");
