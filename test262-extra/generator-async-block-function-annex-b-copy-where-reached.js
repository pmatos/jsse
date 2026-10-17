/*---
description: >
  A function declaration in a block, switch case block, or try/catch/finally
  clause is created when the block is entered, but the Annex B.3.3 copy of its
  binding to the enclosing function's var scope happens only when the
  declaration is reached, even when a `yield` or `await` separates the block's
  entry from the declaration.
esid: sec-web-compat-functiondeclarationinstantiation
info: |
  B.3.2.1 Changes to FunctionDeclarationInstantiation / B.3.2.2 ... Evaluation

  FunctionDeclaration : function BindingIdentifier ( FormalParameters ) { FunctionBody }

  1. Let fenv be the running execution context's VariableEnvironment.
  2. Let benv be the running execution context's LexicalEnvironment.
  3. Let fobj be ! benv.GetBindingValue(F, false).
  4. Perform ? fenv.SetMutableBinding(F, fobj, false).
  5. Return unused.
flags: [async, noStrict]
features: [generators, async-functions, async-iteration]
---*/

var GeneratorFunction = Object.getPrototypeOf(function*() {}).constructor;
var AsyncFunction = Object.getPrototypeOf(async function() {}).constructor;
var AsyncGeneratorFunction = Object.getPrototypeOf(async function*() {}).constructor;

var PRELUDE = "var peek = () => typeof f; var out = [];";

// `@0;` stands for `yield 0;` or `await 0;`. `peek()` reads the function-level
// `f`, which a block-level `f` shadows inside the block.
var cases = [
  ["block",
    "{ out.push(peek()); @0; out.push(peek()); function f() {} out.push(peek()); } out.push(peek());",
    "undefined,undefined,function,function"],
  ["blockWithLet",
    "{ let a = 1; out.push(peek()); @0; function f() { return a; } out.push(peek()); } out.push(peek());",
    "undefined,function,function"],
  ["labelledFunction",
    "{ out.push(peek()); @0; out.push(peek()); l: function f() {} out.push(peek()); } out.push(peek());",
    "undefined,undefined,function,function"],
  ["loopBody",
    "for (var i = 0; i < 2; i++) { out.push(peek()); @0; function f() {} out.push(peek()); }",
    "undefined,function,function,function"],
  ["switchFallsThroughToDeclaration",
    "switch (1) { case 1: out.push(peek()); @0; out.push(peek()); case 2: function f() {} out.push(peek()); }" +
    " out.push(peek());",
    "undefined,undefined,function,function"],
  ["switchSkipsDeclaration",
    "switch (2) { case 1: function f() {} break; case 2: out.push(peek()); @0; out.push(peek()); }" +
    " out.push(peek());",
    "undefined,undefined,undefined"],
  ["switchWithSuspendingTests",
    "switch (@1) { case @1: out.push(peek()); function f() {} out.push(peek()); break; default: out.push('d'); }" +
    " out.push(peek());",
    "undefined,function,function"],
  ["tryBlock",
    "try { out.push(peek()); @0; out.push(peek()); function f() {} out.push(peek()); } finally { out.push('fin'); }" +
    " out.push(peek());",
    "undefined,undefined,function,fin,function"],
  ["catchClause",
    "try { throw 1; } catch (e) { out.push(peek()); @0; out.push(peek()); function f() {} out.push(peek()); }" +
    " out.push(peek());",
    "undefined,undefined,function,function"],
  ["finallyClause",
    "try { } finally { out.push(peek()); @0; out.push(peek()); function f() {} out.push(peek()); }" +
    " out.push(peek());",
    "undefined,undefined,function,function"],
  ["sameFunctionObject",
    "var inner1, inner2; { inner1 = f; @0; function f() {} inner2 = f; } out.push(inner1 === inner2, inner2 === f);",
    "true,true"],
  ["secondDeclarationOfAName",
    "{ out.push(peek()); @0; function f() { return 1; } out.push(peek()); function f() { return 2; } }" +
    " out.push(f());",
    "undefined,function,2"],
];

function variants(src) {
  var body = PRELUDE + src + " return out.join();";
  var out = [];
  out.push(["generator", () => GeneratorFunction(body.replace(/@/g, "yield ")), "sync"]);
  out.push(["generator with yield*", () => GeneratorFunction("if (false) yield* [];" + body.replace(/@/g, "yield ")), "sync"]);
  out.push(["async function", () => AsyncFunction(body.replace(/@/g, "await ")), "async"]);
  out.push(["async generator", () => AsyncGeneratorFunction(body.replace(/@/g, "yield ")), "asyncgen"]);
  return out;
}

function runSync(fn) {
  var it = fn();
  var r = it.next();
  while (!r.done) r = it.next(1);
  return Promise.resolve(r.value);
}

function runAsyncGen(fn) {
  var it = fn();
  function step(r) {
    return r.done ? r.value : it.next(1).then(step);
  }
  return it.next().then(step);
}

var jobs = [];
cases.forEach(function(c) {
  variants(c[1]).forEach(function(v) {
    jobs.push(function() {
      var fn = v[1]();
      var run = v[2] === "sync" ? runSync : v[2] === "async" ? function(f) { return f(); } : runAsyncGen;
      return run(fn).then(function(value) {
        assert.sameValue(value, c[2], c[0] + " (" + v[0] + ")");
      });
    });
  });
});

jobs.push(function() {
  var fn = GeneratorFunction(
    "'use strict'; if (false) yield* []; var peek = () => typeof f; var out = [];" +
    "{ out.push(peek()); yield 0; function f() {} out.push(peek()); } out.push(peek()); return out.join();");
  return runSync(fn).then(function(value) {
    assert.sameValue(value, "undefined,undefined,undefined", "strict mode makes no copy");
  });
});

jobs.reduce(function(p, job) { return p.then(job); }, Promise.resolve()).then($DONE, $DONE);
