/*---
description: >
  for-of inside a function body closes its iterator exactly when the spec's
  IteratorClose applies: on break, on a continue to an outer loop, on return,
  and when the body or the head binding throws; never when the iterator itself
  throws or finishes.
info: |
  jsse issue #960: the VM keeps the iterator as an operand-stack slot and
  routes every exit of the loop through IterClose / IterCloseThrow.
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
includes: [compareArray.js]
---*/

function iterable(log, count, opts) {
  opts = opts || {};
  return {
    [Symbol.iterator]() {
      var i = 0;
      log.push("iterator");
      return {
        next() {
          log.push("next");
          if (opts.throwOnNext !== undefined && i === opts.throwOnNext) throw "next threw";
          return { value: i++, done: i > count };
        },
        return(v) {
          log.push("return");
          if (opts.throwOnReturn) throw "return threw";
          return opts.returnValue === undefined ? {} : opts.returnValue;
        },
      };
    },
  };
}

function exhaustion() {
  var log = [];
  for (var v of iterable(log, 2)) log.push("v" + v);
  return log;
}
assert.compareArray(
  exhaustion(),
  ["iterator", "next", "v0", "next", "v1", "next"],
  "an exhausted iterator is not closed"
);

function breakCloses() {
  var log = [];
  for (var v of iterable(log, 5)) { log.push("v" + v); if (v === 1) break; }
  log.push("after");
  return log;
}
assert.compareArray(
  breakCloses(),
  ["iterator", "next", "v0", "next", "v1", "return", "after"],
  "break calls return()"
);

function returnCloses() {
  var log = [];
  function f() { for (var v of iterable(log, 5)) { return "r" + v; } }
  log.push(f());
  return log;
}
assert.compareArray(returnCloses(), ["iterator", "next", "return", "r0"], "return calls return()");

function labelledContinueClosesInner() {
  var log = [];
  outer: for (var a of [1, 2]) {
    log.push("a" + a);
    for (var b of iterable(log, 3)) { continue outer; }
  }
  return log;
}
assert.compareArray(
  labelledContinueClosesInner(),
  ["a1", "iterator", "next", "return", "a2", "iterator", "next", "return"],
  "continue to an outer loop closes the inner iterator"
);

function labelledBreakClosesBoth() {
  var log = [];
  var outerIter = {
    [Symbol.iterator]() {
      return { next() { return { value: 1, done: false }; }, return() { log.push("outer return"); return {}; } };
    },
  };
  outer: for (var a of outerIter) {
    for (var b of iterable(log, 3)) { break outer; }
  }
  return log;
}
assert.compareArray(
  labelledBreakClosesBoth(),
  ["iterator", "next", "return", "outer return"],
  "break to an outer loop closes inner then outer"
);

function bodyThrowClosesAndKeepsTheThrow() {
  var log = [];
  try {
    for (var v of iterable(log, 3, { throwOnReturn: true })) { throw "body threw"; }
  } catch (e) { log.push(e); }
  return log;
}
assert.compareArray(
  bodyThrowClosesAndKeepsTheThrow(),
  ["iterator", "next", "return", "body threw"],
  "an error from return() does not replace the body's throw"
);

function breakReturnErrorPropagates() {
  var log = [];
  try {
    for (var v of iterable(log, 3, { throwOnReturn: true })) { break; }
  } catch (e) { log.push(e); }
  return log;
}
assert.compareArray(
  breakReturnErrorPropagates(),
  ["iterator", "next", "return", "return threw"],
  "an error from return() propagates after a break"
);

function nonObjectReturnResultThrows() {
  var log = [];
  var caught;
  try {
    for (var v of iterable(log, 3, { returnValue: 1 })) { break; }
  } catch (e) { caught = e; }
  return caught instanceof TypeError;
}
assert.sameValue(nonObjectReturnResultThrows(), true, "return() must produce an object");

function nextThrowDoesNotClose() {
  var log = [];
  try {
    for (var v of iterable(log, 3, { throwOnNext: 1 })) log.push("v" + v);
  } catch (e) { log.push(e); }
  return log;
}
assert.compareArray(
  nextThrowDoesNotClose(),
  ["iterator", "next", "v0", "next", "next threw"],
  "an error from next() does not call return()"
);

function headBindingThrowCloses() {
  var log = [];
  try {
    for (var [a] of iterable(log, 3)) { log.push("unreached"); }
  } catch (e) { log.push(e instanceof TypeError); }
  return log;
}
assert.compareArray(
  headBindingThrowCloses(),
  ["iterator", "next", "return", true],
  "an error while binding the head closes the iterator"
);

function closeThroughFinally() {
  var log = [];
  function f() {
    for (var v of iterable(log, 3)) {
      try { return "ret"; } finally { log.push("finally"); }
    }
  }
  log.push(f());
  return log;
}
assert.compareArray(
  closeThroughFinally(),
  ["iterator", "next", "finally", "return", "ret"],
  "the finalizer runs before the iterator is closed"
);

function finallyBreakSwallowsAndCloses() {
  var log = [];
  for (var v of iterable(log, 3)) {
    try { throw "t"; } finally { break; }
  }
  return log;
}
assert.compareArray(
  finallyBreakSwallowsAndCloses(),
  ["iterator", "next", "return"],
  "break in finally replaces the throw and closes"
);

function perIterationBindings() {
  var fns = [];
  for (let x of [1, 2, 3]) fns.push(function () { return x; });
  for (const [k, v] of [["a", 1], ["b", 2]]) fns.push(function () { return k + v; });
  return fns.map(function (f) { return f(); });
}
assert.compareArray(perIterationBindings(), [1, 2, 3, "a1", "b2"], "closures capture each iteration");

function headBindingForms() {
  var out = [];
  var o = {};
  var x, y;
  for (var a of [1]) out.push(a);
  for (let b of [2]) out.push(b);
  for (const c of [3]) out.push(c);
  for (x of [4]) out.push(x);
  for (o.p of [5]) out.push(o.p);
  for ([x, y] of [[6, 7]]) out.push(x, y);
  for ({ length: y } of ["abc"]) out.push(y);
  for (var { m, n = 10 } of [{ m: 8 }]) out.push(m, n);
  return out;
}
assert.compareArray(
  headBindingForms(),
  [1, 2, 3, 4, 5, 6, 7, 3, 8, 10],
  "head binding forms"
);

function headTDZ() {
  var x = "outer";
  for (let x of [x]) { return "unreached"; }
}
assert.throws(ReferenceError, headTDZ, "the iterated expression sees the head's TDZ binding");

function constHeadIsImmutable() {
  for (const x of [1]) { x = 2; }
}
assert.throws(TypeError, constHeadIsImmutable, "assignment to a const head binding");

function isNotIterable() {
  for (var x of 1) {}
}
assert.throws(TypeError, isNotIterable, "a non-iterable value");

function continueKeepsIterating() {
  var log = [];
  for (var v of iterable(log, 3)) { if (v !== 1) continue; log.push("v" + v); }
  return log;
}
assert.compareArray(
  continueKeepsIterating(),
  ["iterator", "next", "next", "v1", "next", "next"],
  "continue steps to the next value without closing"
);

function returnInNestedLoopsClosesEach() {
  var log = [];
  function f() {
    for (var a of iterable(log, 2)) {
      for (var b of iterable(log, 2)) {
        return [a, b];
      }
    }
  }
  var r = f();
  log.push(r.join());
  return log;
}
assert.compareArray(
  returnInNestedLoopsClosesEach(),
  ["iterator", "next", "iterator", "next", "return", "return", "0,0"],
  "return closes the inner then the outer iterator"
);

function closing(log, name, throwOnReturn) {
  return {
    [Symbol.iterator]() {
      return {
        next() { return { value: 1, done: false }; },
        return() {
          log.push(name + ":return");
          if (throwOnReturn) throw name + " threw";
          return {};
        },
      };
    },
  };
}

function returnErrorIsNotCaughtByTheLoopItClosed() {
  var log = [];
  var it = closing(log, "it", true);
  try { (function () { for (var x of it) { return 1; } })(); } catch (e) { log.push("caught " + e); }
  return log;
}
assert.compareArray(
  returnErrorIsNotCaughtByTheLoopItClosed(),
  ["it:return", "caught it threw"],
  "a return() error on a return exit closes the iterator once, and is not closed again"
);

function breakReturnErrorClosesOnce() {
  var log = [];
  var it = closing(log, "it", true);
  try { for (var i = 0; i < 2; i++) { for (var x of it) { break; } } } catch (e) { log.push("caught " + e); }
  return log;
}
assert.compareArray(
  breakReturnErrorClosesOnce(),
  ["it:return", "caught it threw"],
  "a return() error on a break exit closes once"
);

function outerCloseErrorSkipsTheInnerTry() {
  var log = [];
  var outer = closing(log, "outer", true), inner = closing(log, "inner", false);
  function f() {
    for (var o of outer) {
      try { for (var i of inner) { return 1; } } catch (e) { log.push("inner catch " + e); }
      break;
    }
  }
  try { f(); } catch (e) { log.push("caught " + e); }
  return log;
}
assert.compareArray(
  outerCloseErrorSkipsTheInnerTry(),
  ["inner:return", "outer:return", "caught outer threw"],
  "the return completion has left the try before the outer iterator is closed"
);

function innerCloseErrorIsCaughtByTheEnclosingTry() {
  var log = [];
  var inner = closing(log, "inner", true);
  function f() {
    try { for (var i of inner) { return 1; } } catch (e) { log.push("catch " + e); return 2; }
  }
  log.push(f());
  return log;
}
assert.compareArray(
  innerCloseErrorIsCaughtByTheEnclosingTry(),
  ["inner:return", "catch inner threw", 2],
  "closing the iterator happens inside the try that encloses the loop"
);

function finallyRunsBeforeTheCloseAndTheErrorEscapes() {
  var log = [];
  var outer = closing(log, "outer", true), inner = closing(log, "inner", false);
  var n = 0;
  function f() {
    outer1: for (var o of outer) {
      try { for (var i of inner) { if (n++ === 0) continue outer1; break outer1; } }
      finally { log.push("fin"); }
    }
  }
  try { f(); } catch (e) { log.push("caught " + e); }
  return log;
}
assert.compareArray(
  finallyRunsBeforeTheCloseAndTheErrorEscapes(),
  ["inner:return", "fin", "inner:return", "fin", "outer:return", "caught outer threw"],
  "labelled continue and break through finalizers"
);

function swallowingCatchDoesNotSeeTheOuterCloseError() {
  var log = [];
  var outer = closing(log, "outer", true);
  function f() { for (var o of outer) { try { return 2; } catch (e) { log.push("swallowed"); } } }
  try { f(); } catch (e) { log.push("caught " + e); }
  return log;
}
assert.compareArray(
  swallowingCatchDoesNotSeeTheOuterCloseError(),
  ["outer:return", "caught outer threw"],
  "a return inside try/catch inside for-of"
);

function finallyThenCloseError() {
  var log = [];
  var it = closing(log, "it", true);
  function f() { for (var o of it) { try { return 2; } finally { log.push("fin"); } } }
  try { f(); } catch (e) { log.push("caught " + e); }
  return log;
}
assert.compareArray(
  finallyThenCloseError(),
  ["fin", "it:return", "caught it threw"],
  "the finalizer runs, then the close error propagates"
);
