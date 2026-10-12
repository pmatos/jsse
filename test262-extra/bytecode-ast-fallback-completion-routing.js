/*---
description: >
  break, continue, return and throw completions crossing statements that the
  bytecode compiler hands back to the tree-walker inside an otherwise
  compiled function resume at exactly the spec-mandated place.
info: |
  jsse issue #902: a statement without a native lowering (try, switch,
  for-in, for-of, labeled blocks) runs as one tree-walker node inside a
  compiled body. Its abrupt completions must still reach the enclosing
  compiled loop, finally ordering must hold, and a proper tail call must
  survive the boundary.

  Break statement (14.9.2), continue statement (14.8.2), try statement
  (14.15.3), labelled evaluation (14.13.4), tail position calls (15.10).
esid: sec-break-statement-runtime-semantics-evaluation
flags: [noStrict]
---*/

function continueOutOfTry() {
  var out = [];
  for (var i = 0; i < 4; i++) {
    try {
      if (i === 1) continue;
      out.push("t" + i);
    } finally {
      out.push("f" + i);
    }
  }
  return out.join(",");
}
assert.sameValue(continueOutOfTry(), "t0,f0,f1,t2,f2,t3,f3");

function breakOutOfTryInWhile() {
  var n = 0;
  while (true) {
    n++;
    try {
      if (n >= 3) break;
    } finally {
      n += 10;
    }
  }
  return n;
}
assert.sameValue(breakOutOfTryInWhile(), 22);

function labeledBreakAndContinueOutOfSwitch() {
  var trace = [];
  outer: for (var i = 0; i < 3; i++) {
    for (var j = 0; j < 3; j++) {
      switch (j) {
        case 0:
          continue;
        case 1:
          trace.push("c" + i);
          continue outer;
        default:
          break outer;
      }
    }
  }
  return trace.join(",");
}
assert.sameValue(labeledBreakAndContinueOutOfSwitch(), "c0,c1,c2");

function doWhileContinueRunsTest() {
  var n = 0, ticks = 0;
  do {
    n++;
    try {
      if (n < 3) continue;
    } finally {
      ticks++;
    }
  } while (n < 5);
  return n + ":" + ticks;
}
assert.sameValue(doWhileContinueRunsTest(), "5:5");

function finallyOverridesReturn() {
  try {
    return "try";
  } finally {
    return "finally";
  }
}
assert.sameValue(finallyOverridesReturn(), "finally");

function returnInsideForOf(list) {
  for (var v of list) {
    if (v > 1) return v;
  }
  return -1;
}
assert.sameValue(returnInsideForOf([1, 2, 3]), 2);
assert.sameValue(returnInsideForOf([0]), -1);

function forOfBreakClosesIterator() {
  var closed = 0;
  var iterable = {
    [Symbol.iterator]() {
      return {
        next() { return { value: 1, done: false }; },
        return() { closed++; return {}; }
      };
    }
  };
  var n = 0;
  while (n < 3) {
    for (var v of iterable) {
      n++;
      break;
    }
  }
  return closed + ":" + n;
}
assert.sameValue(forOfBreakClosesIterator(), "3:3");

function throwFromFallbackReachesCaller() {
  var seen;
  try {
    (function () {
      var i = 0;
      while (i < 2) {
        try {
          if (i === 1) throw new RangeError("boom");
        } finally {
          i++;
        }
      }
    })();
  } catch (e) {
    seen = e.name + ":" + e.message;
  }
  return seen;
}
assert.sameValue(throwFromFallbackReachesCaller(), "RangeError:boom");

function labeledBlockBreak() {
  var trace = [];
  for (var i = 0; i < 2; i++) {
    blk: {
      trace.push("a" + i);
      if (i === 0) break blk;
      trace.push("b" + i);
    }
    trace.push("c" + i);
  }
  return trace.join(",");
}
assert.sameValue(labeledBlockBreak(), "a0,c0,a1,b1,c1");

var depth = 0;
function tailRecurse(n) {
  "use strict";
  if (n === 0) return "done";
  try {
    depth++;
  } catch (e) {}
  return tailRecurse(n - 1);
}
assert.sameValue(tailRecurse(50), "done");
