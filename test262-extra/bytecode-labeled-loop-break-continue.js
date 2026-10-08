/*---
description: >
  Labeled while/for loops with labeled break/continue, in the shapes the
  bytecode compiler's labeled-loop lowering must handle identically to the
  tree-walker: the mandreel-style `label: while (true) { ... break label;
  ... continue label; ... }` idiom, a labeled continue reaching past an
  intervening unlabeled inner loop, a for loop's continue still running the
  update expression, and stacked labels on a single loop where either label
  exits it.
info: |
  jsse issue #873: the bytecode compiler's compile_statement previously had
  no arm for Statement::Labeled, Statement::Break, or Statement::Continue,
  so any function body containing one bailed entirely to the tree-walker.
  Mandreel's C-to-JS translation emits every loop as a labeled
  `while(true){...}` using labeled break/continue, so this bail covered
  ~99% of that benchmark's remaining tree-walker work. Every function body
  below stays within what the compiler already accepts (var, if, while,
  for, numeric literals, arithmetic/comparison) so this file exercises the
  new lowering under --bytecode CI runs without needing
  interp.bytecode_enabled set by hand; it does not assert which engine path
  actually ran, since this file runs under both the default and
  --bytecode CI jobs.
esid: sec-runtime-semantics-labelledevaluation
---*/

// The mandreel shape: a labeled while(true) with a matching labeled break
// and a matching labeled continue.
function mandrelShape() {
  var n = 0;
  outer: while (true) {
    n++;
    if (n === 2) continue outer;
    if (n === 4) break outer;
  }
  return n;
}
assert.sameValue(mandrelShape(), 4, "labeled while(true) with break/continue");

// `continue outer` from inside an unlabeled inner loop must skip the rest of
// the inner loop's current iteration and resume the outer loop directly,
// bypassing the inner loop's own continue/exit machinery.
function continuePastInnerLoop() {
  var n = 0;
  outer: for (var i = 0; i < 3; i++) {
    for (var j = 0; j < 3; j++) {
      if (j === 1) continue outer;
      n++;
    }
  }
  return n;
}
assert.sameValue(
  continuePastInnerLoop(),
  3,
  "labeled continue reaches past an intervening unlabeled loop",
);

// ForBodyEvaluation: continue must still run the for loop's update
// expression before the next test, not skip straight back to the test.
function forContinueRunsUpdate() {
  var n = 0;
  for (var i = 0; i < 10; i++) {
    if (i === 2) continue;
    if (i === 5) break;
    n += i;
  }
  return n;
}
assert.sameValue(
  forContinueRunsUpdate(),
  8,
  "for's continue still runs the update expression",
);

// Stacked labels on one loop (`a: b: while`) are one label set on one loop,
// per LabelledEvaluation -- either label must exit the same loop.
function stackedLabelsBreakA() {
  var n = 0;
  a: b: while (n < 5) {
    n++;
    if (n === 3) break a;
  }
  return n;
}
assert.sameValue(stackedLabelsBreakA(), 3, "stacked labels: break by the outer label");

function stackedLabelsBreakB() {
  var n = 0;
  a: b: while (n < 5) {
    n++;
    if (n === 3) break b;
  }
  return n;
}
assert.sameValue(stackedLabelsBreakB(), 3, "stacked labels: break by the inner label");
