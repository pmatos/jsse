/*---
description: >
  A `switch` that is the last statement of a loop body, whose last clause ends in
  an `if` or a labeled block containing `break` or `continue`, must fall out to
  the loop's next iteration instead of looping forever, in a generator where
  another clause of the `switch` contains a `yield`.
esid: sec-runtime-semantics-caseblockevaluation
info: |
  Runtime Semantics: CaseBlockEvaluation

  After the last selected clause completes normally, or with a `break`, the
  CaseBlock completes and evaluation continues with the statement following
  the `switch`; for a `switch` that ends a loop body this is the loop's next
  iteration (sec-while-statement-runtime-semantics-labelledevaluation).
includes: [compareArray.js]
features: [generators]
---*/

function* whileLoop() {
  var log = [];
  var i = 0;
  while (i < 4) {
    i++;
    switch (i) {
      case 1: yield 1; break;
      default: log.push("d" + i); if (i === 2) break;
    }
  }
  log.push("end");
  yield log.join(",");
}

assert.compareArray(Array.from(whileLoop()), [1, "d2,d3,d4,end"], "while loop, tail if-break");

function* forOfLoop() {
  var log = [];
  for (var i of [1, 2, 3]) {
    switch (i) {
      case 1: yield 1; break;
      default: log.push("d" + i); if (i === 2) continue;
    }
  }
  log.push("end");
  yield log.join(",");
}

assert.compareArray(Array.from(forOfLoop()), [1, "d2,d3,end"], "for-of loop, tail if-continue");

function* labeledTail() {
  var log = [];
  var i = 0;
  while (i < 3) {
    i++;
    switch (i) {
      case 1: yield 1; break;
      default: lbl: { log.push("d" + i); break lbl; }
    }
  }
  log.push("end");
  yield log.join(",");
}

assert.compareArray(Array.from(labeledTail()), [1, "d2,d3,end"], "while loop, tail labeled block");

function* nestedInIf() {
  var log = [];
  var i = 0;
  while (i < 3) {
    i++;
    if (i > 0) {
      switch (i) {
        case 1: yield 1; break;
        default: log.push("d" + i); if (i === 2) break;
      }
    }
  }
  log.push("end");
  yield log.join(",");
}

assert.compareArray(Array.from(nestedInIf()), [1, "d2,d3,end"], "switch nested in a loop-tail if");

function* yieldingTail() {
  var log = [];
  var i = 0;
  while (i < 3) {
    i++;
    switch (i) {
      case 1: log.push("one"); break;
      default: yield i; if (i === 2) break;
    }
  }
  log.push("end");
  yield log.join(",");
}

assert.compareArray(Array.from(yieldingTail()), [2, 3, "one,end"], "while loop, yielding tail clause");
