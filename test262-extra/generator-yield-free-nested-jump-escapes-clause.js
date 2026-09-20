/*---
description: >
  In a generator, a yield-free statement that contains nested loops or
  switches must still deliver a labeled `break`/`continue` that targets a
  construct outside it, while a jump that the nested construct consumes itself
  must stay local to it.
esid: sec-runtime-semantics-labelledevaluation
info: |
  LabelledStatement evaluation consumes a `break` completion whose target is
  the statement's own label; LoopContinues (sec-loopcontinues) consumes a
  `continue` completion whose target is empty or in the loop's label set. Any
  other break/continue completion propagates to the enclosing statement, and
  ends CaseBlockEvaluation (sec-runtime-semantics-caseblockevaluation).
includes: [compareArray.js]
features: [generators]
---*/

function run(gen) {
  return Array.from(gen);
}

function* breakOuterFromNestedLoop(x) {
  var l = [];
  outer: for (var i = 0; i < 2; i++) {
    switch (x) {
      case 1: for (;;) { l.push("in" + i); break outer; }
      case 2: l.push("two"); break;
      case 3: yield 0;
    }
    l.push("after" + i);
  }
  yield l.join();
}

assert.compareArray(run(breakOuterFromNestedLoop(1)), ["in0"], "break outer from a nested for");
assert.compareArray(run(breakOuterFromNestedLoop(2)), ["two,after0,two,after1"], "case 2");

function* continueOuterFromNestedSwitch(x) {
  var l = [];
  outer: for (var i = 0; i < 3; i++) {
    switch (x) {
      case 1:
        switch (i) { case 0: continue outer; }
        l.push("c1-" + i);
        break;
      case 2: l.push("two"); break;
      case 3: yield 0;
    }
    l.push("after" + i);
  }
  yield l.join();
}

assert.compareArray(
  run(continueOuterFromNestedSwitch(1)),
  ["c1-1,after1,c1-2,after2"],
  "continue outer from a nested switch"
);

function* nativeJumpsStayLocal(x) {
  var l = [];
  switch (x) {
    case 1:
      while (1) { l.push("w"); break; }
      for (var i = 0; i < 2; i++) { if (i == 0) continue; l.push("i" + i); }
      inner: { l.push("b"); break inner; }
      l.push("post");
      break;
    case 2: l.push("two"); break;
    case 3: yield 0;
  }
  yield l.join();
}

assert.compareArray(
  run(nativeJumpsStayLocal(1)),
  ["w,i1,b,post"],
  "jumps consumed by a nested loop or label do not leave the clause"
);

function* labeledBlockAroundSwitch(x) {
  var l = [];
  outer: {
    switch (x) {
      case 1: try { break outer; } finally { l.push("f"); }
      case 2: l.push("two"); break;
      case 3: yield 0;
    }
    l.push("inside");
  }
  l.push("after");
  yield l.join();
}

assert.compareArray(
  run(labeledBlockAroundSwitch(1)),
  ["f,after"],
  "break to a labeled block from a try in a clause"
);
