/*---
description: >
  try/catch/finally inside a function body: a break, continue, return or throw
  leaving the try block or the catch block runs the finalizer first, and an
  abrupt completion of the finalizer replaces the pending one.
info: |
  jsse issue #960: compiled bodies lower `try` to a handler table and a
  finalizer that completion records (normal, throw, break/continue/return)
  re-enter, instead of handing the statement to the tree-walker.
esid: sec-try-statement-runtime-semantics-evaluation
includes: [compareArray.js]
---*/

function normal() {
  var log = [];
  try { log.push("try"); } catch (e) { log.push("catch"); } finally { log.push("finally"); }
  log.push("after");
  return log;
}
assert.compareArray(normal(), ["try", "finally", "after"], "normal completion");

function caught() {
  var log = [];
  try { log.push("try"); throw "x"; log.push("unreached"); }
  catch (e) { log.push("catch " + e); }
  finally { log.push("finally"); }
  return log;
}
assert.compareArray(caught(), ["try", "catch x", "finally"], "throw is caught");

function returnRunsFinally() {
  var log = [];
  function f() {
    try { return log.push("return value") && "r"; } finally { log.push("finally"); }
  }
  assert.sameValue(f(), "r");
  return log;
}
assert.compareArray(returnRunsFinally(), ["return value", "finally"], "return");

function returnValueIsFixedBeforeFinally() {
  var x = 1;
  function f() {
    try { return x; } finally { x = 2; }
  }
  return [f(), x];
}
assert.compareArray(returnValueIsFixedBeforeFinally(), [1, 2], "return value read before finally");

function finallyOverridesReturn() {
  try { return "try"; } finally { return "finally"; }
}
assert.sameValue(finallyOverridesReturn(), "finally", "return in finally replaces return");

function finallyOverridesThrow() {
  try { throw new Error("lost"); } finally { return "finally"; }
}
assert.sameValue(finallyOverridesThrow(), "finally", "return in finally discards the throw");

function breakInFinallyDiscardsThrow() {
  var log = [];
  for (var i = 0; i < 3; i++) {
    try { log.push("try " + i); throw "boom"; }
    finally { log.push("finally " + i); break; }
  }
  return log;
}
assert.compareArray(
  breakInFinallyDiscardsThrow(),
  ["try 0", "finally 0"],
  "break in finally discards the throw"
);

function continueInFinallyDiscardsReturn() {
  var log = [];
  function f() {
    for (var i = 0; i < 2; i++) {
      try { return "r" + i; } finally { log.push("finally " + i); continue; }
    }
    return "end";
  }
  return [f()].concat(log);
}
assert.compareArray(
  continueInFinallyDiscardsReturn(),
  ["end", "finally 0", "finally 1"],
  "continue in finally discards the return"
);

function throwInFinallyReplaces() {
  try {
    try { throw "first"; } finally { throw "second"; }
  } catch (e) {
    return e;
  }
}
assert.sameValue(throwInFinallyReplaces(), "second", "throw in finally replaces the pending throw");

function throwInCatchRunsFinally() {
  var log = [];
  try {
    try { throw "a"; } catch (e) { log.push("catch " + e); throw "b"; } finally { log.push("finally"); }
  } catch (e) {
    log.push("outer " + e);
  }
  return log;
}
assert.compareArray(
  throwInCatchRunsFinally(),
  ["catch a", "finally", "outer b"],
  "a throw from the catch block runs the finalizer"
);

function nestedFinalizersRunInnerFirst() {
  var log = [];
  function f() {
    try {
      try {
        try { return "v"; } finally { log.push("inner"); }
      } finally { log.push("middle"); }
    } finally { log.push("outer"); }
  }
  log.unshift(f());
  return log;
}
assert.compareArray(
  nestedFinalizersRunInnerFirst(),
  ["v", "inner", "middle", "outer"],
  "return runs every enclosing finalizer, innermost first"
);

function breakAndContinueThroughFinalizers() {
  var log = [];
  outer: for (var i = 0; i < 3; i++) {
    try {
      for (var j = 0; j < 3; j++) {
        try {
          if (j === 1) continue outer;
          if (i === 2) break outer;
          log.push(i + ":" + j);
        } finally { log.push("f-in " + i + j); }
      }
    } finally { log.push("f-out " + i); }
  }
  return log;
}
assert.compareArray(
  breakAndContinueThroughFinalizers(),
  [
    "0:0", "f-in 00", "f-in 01", "f-out 0",
    "1:0", "f-in 10", "f-in 11", "f-out 1",
    "f-in 20", "f-out 2",
  ],
  "labeled break and continue run the finalizers they cross"
);

function loopsRunFinalizersEveryIteration() {
  var log = [];
  for (var i = 0; i < 3; i++) {
    try {
      if (i === 0) continue;
      if (i === 1) { log.push("body"); }
    } finally { log.push("f" + i); }
    log.push("after " + i);
  }
  return log;
}
assert.compareArray(
  loopsRunFinalizersEveryIteration(),
  ["f0", "body", "f1", "after 1", "f2", "after 2"],
  "continue inside try"
);

function catchBindingScope() {
  var e = "outer";
  var seen = [];
  try { throw "inner"; } catch (e) { seen.push(e); var hoisted = e; e = "assigned"; seen.push(e); }
  seen.push(e);
  seen.push(hoisted);
  return seen;
}
assert.compareArray(
  catchBindingScope(),
  ["inner", "assigned", "outer", "inner"],
  "the catch parameter shadows and does not leak"
);

function catchVarRedeclaresParameter() {
  try { throw 1; } catch (e) { var e = 2; return e; }
}
assert.sameValue(catchVarRedeclaresParameter(), 2, "var e initialises the catch parameter");

function catchDestructuring() {
  try { throw { a: 1, b: [2, 3] }; } catch ({ a, b: [c, d = 9] }) { return [a, c, d]; }
}
assert.compareArray(catchDestructuring(), [1, 2, 3], "destructuring catch parameter");

function optionalCatchBinding() {
  try { throw 1; } catch { return "no binding"; }
}
assert.sameValue(optionalCatchBinding(), "no binding", "catch without a parameter");

function catchBindingTDZ() {
  try { throw 0; } catch ({ a = b, b }) { return "unreached"; }
}
assert.throws(ReferenceError, catchBindingTDZ, "destructuring default reads a later binding in its TDZ");

function blockScopedInTry() {
  let x = "function";
  try { let x = "try"; throw x; } catch (e) { return [e, x]; } finally { let x = "finally"; }
}
assert.compareArray(blockScopedInTry(), ["try", "function"], "block scopes of try/catch/finally");

function closuresCaptureCatchBinding() {
  var fns = [];
  for (var i = 0; i < 3; i++) {
    try { throw i; } catch (e) { fns.push(function () { return e; }); }
  }
  return fns.map(function (f) { return f(); });
}
assert.compareArray(closuresCaptureCatchBinding(), [0, 1, 2], "each catch has its own binding");

function finallyThrowsWhileReturning() {
  var log = [];
  function f() {
    try { return "r"; } finally { log.push("f"); throw "t"; }
  }
  try { f(); } catch (e) { log.push("caught " + e); }
  return log;
}
assert.compareArray(finallyThrowsWhileReturning(), ["f", "caught t"], "throw in finally replaces return");

function throwFromNestedCalls() {
  function thrower(n) { if (n === 0) throw "deep"; return thrower(n - 1) + 1; }
  var log = [];
  for (var i = 0; i < 2; i++) {
    try { thrower(5); } catch (e) { log.push(e + i); } finally { log.push("f" + i); }
  }
  return log;
}
assert.compareArray(throwFromNestedCalls(), ["deep0", "f0", "deep1", "f1"], "throw from nested calls");

function stackIsCutBackAtTheHandler() {
  function boom() { throw new Error("x"); }
  var o = { m: function (a, b) { return a + b; } };
  var r = 0;
  for (var i = 0; i < 3; i++) {
    try { r += o.m(1, [2, 3, boom()].length); } catch (e) { r += 10; }
  }
  return r;
}
assert.sameValue(stackIsCutBackAtTheHandler(), 30, "operands of a failed expression are discarded");

function tryInExpressionOperandsContext() {
  function g() { try { return 1; } finally { } }
  return g() + g() * 2;
}
assert.sameValue(tryInExpressionOperandsContext(), 3, "try in a callee used within an expression");
