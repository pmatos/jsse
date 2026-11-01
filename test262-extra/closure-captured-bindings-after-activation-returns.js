/*---
description: A closure still reads every binding it names after the activation that created it has returned
info: |
  10.2.1.1 PrepareForOrdinaryCall / 9.1.2.4 NewFunctionEnvironment
  9.1.1.1.6 GetBindingValue (declarative Environment Record)
  10.2.11 FunctionDeclarationInstantiation

  A function object keeps the Environment Record it was created in
  ([[Environment]]), and every binding of it stays readable and writable by
  the closure after the call that created the record has returned, however
  the closure names it: a plain read, an assignment, an update, a shorthand
  property, a destructuring default, a template substitution, a nested
  closure, a class member, or a direct eval. The engine may drop bindings no
  closure can name once the activation is over; none of these may be lost.
esid: sec-functiondeclarationinstantiation
flags: [noStrict]
---*/

function plainRead(a, b) {
  var local = { tag: "local" };
  let lexical = [1, 2, 3];
  const k = function () { return a + ":" + local.tag + ":" + lexical.length; };
  var dead = { tag: "dead" };
  return k;
}
assert.sameValue(plainRead("p", "q")(), "p:local:3", "read of parameter, var and let");

function writes() {
  var count = 0;
  var unused = {};
  return { inc: function () { return ++count; }, set: function (v) { count = v; } };
}
var w = writes();
w.inc();
w.set(41);
assert.sameValue(w.inc(), 42, "assignment and update through a closure");

function nestedDeep(x) {
  var y = { v: 1 };
  var skipped = { v: 100 };
  return function () {
    return function () {
      return function () { return x + y.v; };
    };
  };
}
assert.sameValue(nestedDeep(10)()()(), 11, "a name used three closures down");

function shorthandAndPatterns(a, b) {
  var o = { a, b };
  var gone = {};
  return function ({ c = a } = {}, [d = b] = []) { return [c, d, o]; };
}
var sp = shorthandAndPatterns("A", "B");
assert.sameValue(sp()[0] + sp()[1], "AB", "destructuring defaults read the outer parameters");

function templates(name) {
  var n = 3;
  var unused = {};
  return function () { return `${name}${n}`; };
}
assert.sameValue(templates("t")(), "t3", "template substitution");

function inClass(base) {
  var offset = 5;
  var unused = {};
  class C extends base {
    static s = offset;
    f = offset + 1;
    m() { return offset + 2; }
    static { C.blockSeen = offset + 3; }
    get g() { return offset + 4; }
    [offset + "k"]() { return "computed"; }
  }
  return C;
}
var C = inClass(Object);
var c = new C();
assert.sameValue(C.s + c.f + c.m() + C.blockSeen + c.g, 5 + 6 + 7 + 8 + 9, "class members name the outer variable");
assert.sameValue(c["5k"](), "computed", "computed key evaluated in the activation");

function directEvalInNested() {
  var hidden = "found";
  var other = {};
  return function (src) { return eval(src); };
}
assert.sameValue(directEvalInNested()("hidden"), "found", "eval in a nested function reaches every outer binding");

function directEvalInOuter() {
  var hidden = "found-outer";
  eval("var viaEval = function () { return hidden; };");
  return viaEval;
}
assert.sameValue(directEvalInOuter()(), "found-outer", "a closure made by eval keeps the activation's bindings");

function withStatement(o) {
  var kept = "kept";
  with (o) {
    return function () { return kept + p; };
  }
}
assert.sameValue(withStatement({ p: "!" })(), "kept!", "closure inside with");

function mappedArguments(a, b) {
  var f = function () { return a + b; };
  mappedArguments.saved = arguments;
  return f;
}
var mf = mappedArguments(1, 2);
assert.sameValue(mappedArguments.saved[0], 1, "mapped arguments read the parameter after return");
mappedArguments.saved[1] = 40;
assert.sameValue(mf(), 41, "arguments write is seen by the closure");

function escapedFunctionArguments(a) {
  var g = function () { return typeof a; };
  return function () { return escapedFunctionArguments.arguments; };
}
assert.sameValue(typeof escapedFunctionArguments(1)(), "object", "f.arguments does not throw");

function withDefaults(a, b = a + 1, c = function () { return a + b; }) {
  var unused = {};
  var d = 4;
  return function () { return c() + d; };
}
assert.sameValue(withDefaults(1)(), 1 + 2 + 4, "parameter expressions and a closure in a default");

function withDefaultsSeparateBody(a, b = () => a) {
  var a = 10;
  var unused = {};
  return function () { return [a, b()]; };
}
var sb = withDefaultsSeparateBody(1)();
assert.sameValue(sb[0] + ":" + sb[1], "10:1", "separate parameter and body environments");

function* generated(x) {
  var y = { v: 2 };
  var unused = {};
  yield function () { return x + y.v; };
}
assert.sameValue(generated(1).next().value(), 3, "a closure made inside a generator");

function arrowThis() {
  var unused = {};
  return () => this.tag;
}
assert.sameValue(arrowThis.call({ tag: "this-kept" })(), "this-kept", "arrow reads the activation's this");

function arrowArguments() {
  var unused = {};
  return () => arguments[0];
}
assert.sameValue(arrowArguments("arg0")(), "arg0", "arrow reads the activation's arguments");

function recursiveLocal(n) {
  function fact(k) { return k <= 1 ? 1 : k * fact(k - 1); }
  var unused = {};
  return function () { return fact(n); };
}
assert.sameValue(recursiveLocal(5)(), 120, "a function declaration named only by itself and the closure");

function loops() {
  var fns = [];
  for (let i = 0; i < 3; i++) {
    let j = i * 2;
    var scratch = { i };
    fns.push(() => i + j);
  }
  return fns;
}
assert.sameValue(loops().map(function (f) { return f(); }).join(), "0,3,6", "per-iteration bindings");

function usesTypeofAndDelete() {
  var present = 1;
  var unused = {};
  return function () { return typeof present + typeof absentEverywhere; };
}
assert.sameValue(usesTypeofAndDelete()(), "numberundefined", "typeof of a captured and of an unresolvable name");

var calls = 0;
function repeated(tag) {
  var data = { tag };
  return function () { calls++; return data.tag; };
}
var first = repeated("one");
var second = repeated("two");
assert.sameValue(first() + second() + first(), "onetwoone", "separate activations keep separate bindings");

function constructed() {
  this.keep = { v: 7 };
  var unused = { v: 0 };
  var self = this;
  this.read = function () { return self.keep.v; };
}
assert.sameValue(new constructed().read(), 7, "constructor activation");

var strictResult = (function () {
  "use strict";
  var live = { v: "strict-live" };
  var dead = { v: "strict-dead" };
  return function () { return live.v; };
})();
assert.sameValue(strictResult(), "strict-live", "strict function");
