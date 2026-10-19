// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-async-generator-function-definitions
description: >
  Declarations, expressions, object and class methods (super, private fields,
  static), default and destructured parameters, arguments, this, hoisting,
  closures over per-iteration bindings, labeled loops, switch, yield* in a
  state-machine body, consumption by for await, and a throwing parameter
  initializer behave identically when the body is a suspending bytecode chunk.
info: |
  Parameters are bound, and their errors thrown, when the generator function
  is called; the body starts at the first next().
flags: [async, noStrict]
includes: [compareArray.js]
features: [async-iteration]
---*/

var log = [];
var EXPECTED = [
  "decl:a,b,2",
  "expr:function,z",
  "method:obj,m",
  "computed:c1",
  "derived:base+d,priv",
  "static:sbase+sd",
  "defaults:1,2,,0",
  "closures:0,1,2,012",
  "args:mutated/y",
  "strictThis:7",
  "sloppyThis:object",
  "hoisting:function,function,1",
  "labeled:00,10",
  "switch1:one,two switch3:other",
  "conditional:odd3,even2,odd1,even0",
  "infinite:0,1,2,3,4",
  "paramsThrow:param",
  "tdz:ReferenceError,1",
  "star:1,2,3,4,5",
  "proto:true",
  "tag:[object AsyncGenerator] true",
  "fromSync:1,2",
  "new:TypeError"
];
function later(value) { return new Promise(function (resolve) { Promise.resolve().then(function () { resolve(value); }); }); }
async function collect(it, limit) {
  var out = [];
  for await (var v of it) { out.push(v); if (out.length >= (limit || 100)) break; }
  return out.join(",");
}
async function* declaration(a, b) { yield a; yield b; yield arguments.length; }
var expression = async function* named(a) { yield typeof named; yield a; };
var obj = {
  tag: "obj",
  async *method() { yield this.tag; yield await later("m"); },
  async *["computed" + 1]() { yield "c1"; },
};
class Base { async *hello() { yield "base"; } static async *sbase() { yield "sbase"; } }
class Derived extends Base {
  #p = "priv";
  async *hello() { for await (var x of super.hello()) yield x + "+d"; yield this.#p; }
  static async *sd() { for await (var x of super.sbase()) yield x + "+sd"; }
}
async function* defaults(a = 1, { b = 2, c } = {}, ...rest) { yield [a, b, c, rest.length].join(); }
async function* closures() {
  var fns = [];
  for (let i = 0; i < 3; i++) { await later(0); fns.push(function () { return i; }); yield i; }
  yield fns.map(function (f) { return f(); }).join("");
}
async function* argumentsMapped() { arguments[0] = "mutated"; await later(0); yield Array.prototype.join.call(arguments, "/"); }
async function* strictThis() { "use strict"; yield String(this); }
async function* sloppyThis() { yield typeof this; }
async function* hoisting() { var before = typeof f; yield before; function f() {} yield typeof f; var late = 1; yield late; }
async function* labeled() {
  outer: for (var i = 0; i < 3; i++) { for (var j = 0; j < 3; j++) { if (j == 1) continue outer; if (i == 2) break outer; yield i + "" + j; } }
}
async function* switching(x) { switch (x) { case 1: yield "one"; case 2: yield "two"; break; default: yield "other"; } }
async function* conditional(n) { while (n-- > 0) { if (n % 2) yield "odd" + n; else { yield* []; yield "even" + n; } } }
async function* infinite() { var i = 0; while (true) yield i++; }
async function* paramsThrow(a = (function () { throw new Error("param"); })()) { yield 1; }
async function* tdz() { try { x; } catch (e) { yield e.name; } let x = 1; yield x; }
async function* withYieldStar() { yield 1; yield* [2, 3]; yield* (async function* () { yield 4; })(); yield 5; }
function* syncGen() { yield 1; yield 2; }

(async function () {
  log.push("decl:" + await collect(declaration("a", "b")));
  log.push("expr:" + await collect(expression("z")));
  log.push("method:" + await collect(obj.method()));
  log.push("computed:" + await collect(obj.computed1()));
  log.push("derived:" + await collect(new Derived().hello()));
  log.push("static:" + await collect(Derived.sd()));
  log.push("defaults:" + await collect(defaults()));
  log.push("closures:" + await collect(closures()));
  log.push("args:" + await collect(argumentsMapped("x", "y")));
  log.push("strictThis:" + await collect(strictThis.call(7)));
  log.push("sloppyThis:" + await collect(sloppyThis.call(7)));
  log.push("hoisting:" + await collect(hoisting()));
  log.push("labeled:" + await collect(labeled()));
  log.push("switch1:" + await collect(switching(1)) + " switch3:" + await collect(switching(3)));
  log.push("conditional:" + await collect(conditional(4)));
  log.push("infinite:" + await collect(infinite(), 5));
  try { paramsThrow(); log.push("no-throw"); } catch (e) { log.push("paramsThrow:" + e.message); }
  log.push("tdz:" + await collect(tdz()));
  log.push("star:" + await collect(withYieldStar()));
  log.push("proto:" + (Object.getPrototypeOf(declaration) === Object.getPrototypeOf(async function* () {})));
  var it = declaration(1, 2);
  log.push("tag:" + Object.prototype.toString.call(it) + " " + (it[Symbol.asyncIterator]() === it));
  log.push("fromSync:" + await collect((async function* () { yield* syncGen(); })()));
  try { new declaration(); } catch (e) { log.push("new:" + e.constructor.name); }
})().then(function () {
  assert.compareArray(log, EXPECTED);
}).then($DONE, $DONE);
