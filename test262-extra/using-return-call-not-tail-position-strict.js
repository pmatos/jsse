// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-block-runtime-semantics-evaluation
description: >
  A call returned from a scope that holds a `using` declaration is evaluated
  before the scope's resources are disposed, also in strict code.
info: |
  Block : { StatementList }

  [...]
  4. Let result be Completion(Evaluation of StatementList).
  5. Set result to Completion(DisposeResources(blockEnv.[[DisposeCapability]], result)).

  `return f()` completes with the value of `f()`; DisposeResources runs on
  that completion, so the call cannot be a tail call out of the scope.
flags: [onlyStrict]
features: [explicit-resource-management, tail-call-optimization]
includes: [compareArray.js]
---*/

"use strict";

var log = [];
function leaf() { log.push("call"); return 1; }
var resource = { [Symbol.dispose]() { log.push("dispose"); } };

function check(fn, expected, label) {
  log = [];
  assert.sameValue(fn(), 1, label + " result");
  assert.compareArray(log, expected, label);
}

check(function () { { using r = resource; return leaf(); } }, ["call", "dispose"], "block");
check(function () { using r = resource; return leaf(); }, ["call", "dispose"], "function level");
check(function () { eval(""); { using r = resource; return leaf(); } }, ["call", "dispose"], "block, eval body");
check(function () { eval(""); using r = resource; return leaf(); }, ["call", "dispose"], "function level, eval body");
check(function () { for (;;) { using r = resource; return leaf(); } }, ["call", "dispose"], "loop body");
check(function () { for (using r = resource; ; ) { return leaf(); } }, ["call", "dispose"], "for head");
check(function () { for (var x of [0]) { using r = resource; return leaf(); } }, ["call", "dispose"], "for-of body");
check(function () { { using r = resource; { return leaf(); } } }, ["call", "dispose"], "nested block");
check(function () { { using r = resource; if (true) return leaf(); } }, ["call", "dispose"], "if in block");
check(function () { { using r = resource; return true ? leaf() : 0; } }, ["call", "dispose"], "conditional");
check(function () { { using r = resource; return 0 || leaf(); } }, ["call", "dispose"], "logical or");
check(function () { { using r = resource; return (0, leaf()); } }, ["call", "dispose"], "comma");
check(function () { { using r = resource; return log.push("x") && leaf(); } }, ["x", "call", "dispose"], "logical and");
check(function () { try { using r = resource; return leaf(); } finally { log.push("finally"); } }, ["call", "dispose", "finally"], "try block");
check(function () { try { throw 0; } catch (e) { { using r = resource; return leaf(); } } }, ["call", "dispose"], "block in catch");

check(function () {
  using a = { [Symbol.dispose]() { log.push("a"); } };
  using b = { [Symbol.dispose]() { log.push("b"); } };
  return leaf();
}, ["call", "b", "a"], "two resources dispose in reverse order");

var method = { m() { using r = resource; return leaf(); } };
check(function () { return method.m(); }, ["call", "dispose"], "method");

var arrow = () => { using r = resource; return leaf(); };
check(arrow, ["call", "dispose"], "arrow function");

var thrower = { [Symbol.dispose]() { log.push("dispose"); throw new Test262Error("disposer"); } };
log = [];
assert.throws(Test262Error, function () { using r = thrower; return leaf(); }, "throwing disposer");
assert.compareArray(log, ["call", "dispose"], "throwing disposer runs after the call");

log = [];
assert.throws(SyntaxError, function () {
  using r = resource;
  return (function () { log.push("callee"); throw new SyntaxError("callee"); })();
}, "throwing callee");
assert.compareArray(log, ["callee", "dispose"], "a throwing callee still disposes");

function Ctor() {
  using r = resource;
  return leaf();
}
log = [];
assert.sameValue(typeof new Ctor(), "object", "constructor ignores a primitive return");
assert.compareArray(log, ["call", "dispose"], "constructor");

function count(n, acc) {
  if (n <= 0) return acc;
  return count(n - 1, acc + 1);
}
assert.sameValue(count(200000, 0), 200000, "a proper tail call without a using declaration remains optimized");
