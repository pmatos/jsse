// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  The heap-allocated value a for-of loop carries out through an abrupt exit
  (return, break, or continue-to-an-outer-label) stays reachable across the
  garbage collection that can happen while the loop's IteratorClose runs
  return(), even though nothing but the completion itself references the
  value. break/continue completion values are only observable through
  Eval's completion value (UpdateEmpty), so these cases are driven through
  `eval`. A continue to the loop's *own* label skips IteratorClose entirely
  (LoopContinues is true), but the same running value must still survive
  the next iteration's call to the iterator's next(), which can likewise
  run arbitrary user code that collects.
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind,
  lhsKind, labelSet [ , iteratorKind ] )

  Each abrupt-exit case (break, return, continue to an outer label) calls
  IteratorClose with the loop's own iterator before returning the
  completion that carries the value produced by the loop body. A continue
  to the loop's own label does not call IteratorClose — LoopContinues is
  true — so the loop instead re-enters IteratorStep, whose call to next()
  is the next point arbitrary code (and thus a collection) can run.
features: [host-gc-required]
---*/

function collect() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

function makeIterable() {
  return {
    [Symbol.iterator]() {
      var i = 0;
      return {
        next() {
          i++;
          return { done: false, value: i };
        },
        return(v) {
          collect();
          return { done: true };
        },
      };
    },
  };
}

// Unlike makeIterable(), forces the collection from next() itself (used by
// the no-IteratorClose case below) rather than from return().
function makeSelfContinueIterable() {
  return {
    [Symbol.iterator]() {
      var i = 0;
      return {
        next() {
          i++;
          if (i === 2) {
            collect();
            return { done: true };
          }
          return { done: false, value: i };
        },
      };
    },
  };
}

// (i) `return` from inside the loop body.
function returnFromLoop() {
  for (const x of makeIterable()) {
    return { tag: "return-payload" };
  }
}
var returned = returnFromLoop();
assert.sameValue(
  returned.tag,
  "return-payload",
  "return value out of for-of survives IteratorClose's return() call"
);

// (ii)-(iv): break/continue carry no JS-visible value of their own, but the
// engine threads the loop body's last Normal completion value through
// UpdateEmpty for Eval's sake — observable only via `eval`'s own
// completion value.
var broke = eval(
  "for (const x of makeIterable()) { ({ tag: 'break-payload' }); break; }"
);
assert.sameValue(
  broke.tag,
  "break-payload",
  "value threaded through an unlabeled break survives IteratorClose's return() call"
);

var labeledBroke = eval(
  "outer: for (const x of makeIterable()) { ({ tag: 'labeled-break-payload' }); break outer; }"
);
assert.sameValue(
  labeledBroke.tag,
  "labeled-break-payload",
  "value threaded through a labeled break survives IteratorClose's return() call"
);

var continued = eval(
  "outer: for (const o of [1]) { for (const x of makeIterable()) { ({ tag: 'continue-payload' }); continue outer; } }"
);
assert.sameValue(
  continued.tag,
  "continue-payload",
  "value threaded through a continue to an outer label survives the inner loop's IteratorClose return() call"
);

// (v) a continue to the loop's *own* label does not call IteratorClose at
// all (LoopContinues is true), but the engine must still keep the running
// value rooted across the collection that the next iteration's call to
// next() can trigger. makeSelfContinueIterable() forces that collection
// deterministically (no GC-stress sampling needed to hit the window).
var selfContinued = eval(
  "outer: for (const o of makeSelfContinueIterable()) { ({ tag: 'self-continue-payload' }); continue outer; }"
);
assert.sameValue(
  selfContinued.tag,
  "self-continue-payload",
  "value threaded through a continue to the loop's own label survives the next iteration's call to next()"
);

// (vi) the same shape repeated across several iterations, to catch a
// GC-root-stack imbalance even without GC stress (see release-checked gate).
var multiContinued = eval(
  "outer: for (const o of [1, 2, 3]) { if (o === 2) { ({ t: 2 }); continue outer; } ({ t: o }); }"
).t;
assert.sameValue(
  multiContinued,
  3,
  "running value survives repeated same-label continues"
);
