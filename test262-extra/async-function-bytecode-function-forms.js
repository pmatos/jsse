// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-async-function-definitions
description: >
  Declarations, expressions, arrows (default and rest parameters), methods,
  computed and class methods (super, private fields, static), destructured
  parameters, arguments, this, hoisting, closures over per-iteration bindings,
  recursion, a throwing parameter initializer and a synchronous first part,
  all behave identically when the body is a suspending bytecode chunk.
info: |
  AsyncFunctionBody runs synchronously until its first Await; parameters are
  bound (and their errors reject the promise) before the body starts.
flags: [async, noStrict]
includes: [compareArray.js]
features: [async-functions]
---*/

var log = [];
var EXPECTED = [
  "0=>5",
  "1=>function!",
  "2=>1,5,0",
  "3=>1,2,2",
  "4=>42",
  "5=>1,2,3,4",
  "6=>obj",
  "7=>obj",
  "8=>c1",
  "9=>base+derived",
  "10=>s",
  "11=>static-base+sd",
  "12=>Derived",
  "13=>mutated/b",
  "14=>object",
  "15=>undefined",
  "16=>7",
  "17=>1,2",
  "18=>function,h,later",
  "19=>2:012",
  "20=>25",
  "21!!param",
  "sync-part",
  "after-call",
  "async-part",
  "22=>undefined",
  "23=>1",
  "24=>2",
  "25=>true",
  "construct:TypeError",
  true,
  "2,1,false"
];
function later(value) { return new Promise(function (resolve) { Promise.resolve().then(function () { resolve(value); }); }); }

async function declaration(a, b) { var s = a + b; await later(0); return s + arguments.length; }
var expression = async function named(a) { await later(0); return typeof named + a; };
var arrow = async (a, b = 5, ...rest) => { await later(0); return [a, b, rest.length].join(); };
var arrowExpr = async x => await later(x * 2);
var destructured = async function ({ a, b = 2 }, [c, d = 4]) { await later(0); return [a, b, c, d].join(); };
var obj = {
  tag: "obj",
  async method() { await later(0); return this.tag; },
  async arrowThis() { var f = async () => { await later(0); return this.tag; }; return await f(); },
  async ["computed" + 1]() { return "c1"; },
};
class Base { async hello() { await later(0); return "base"; } static async sbase() { return "static-base"; } }
class Derived extends Base {
  #secret = "s";
  async hello() { var b = await super.hello(); return b + "+derived"; }
  async priv() { await later(0); return this.#secret; }
  static async sderived() { return (await super.sbase()) + "+sd"; }
  async fieldClosure() { var self = this; return await (async function () { await later(0); return self.constructor.name; })(); }
}
async function argumentsAcrossAwait() { await later(0); arguments[0] = "mutated"; await later(0); return Array.prototype.join.call(arguments, "/"); }
async function sloppyThis() { await later(0); return typeof this; }
async function strictThis() { "use strict"; await later(0); return String(this); }
async function paramsAreBoundBeforeBody(a, b = a + 1) { var a; await later(0); return [a, b].join(); }
async function varAndFunctionHoisting() {
  var before = typeof hoisted;
  await later(0);
  function hoisted() { return "h"; }
  var late = "later";
  return before + "," + hoisted() + "," + late;
}
async function closuresShareBindings() {
  var count = 0;
  var inc = function () { return ++count; };
  await later(0); inc();
  await later(0); inc();
  var fns = [];
  for (let i = 0; i < 3; i++) { await later(0); fns.push(function () { return i; }); }
  return count + ":" + fns.map(function (f) { return f(); }).join("");
}
async function recursion(n) { if (n === 0) return 0; return 1 + await recursion(n - 1); }
async function parametersThrow(a = (function () { throw new Error("param"); })()) { return a; }
async function syncPart() { log.push("sync-part"); await 0; log.push("async-part"); }
async function awaitInFinallyOnly() { try { return 1; } finally { await later(0); } }
async function evalInside() { await later(0); return eval("1 + 1"); }
async function newTargetFree() { return new.target === undefined; }

var steps = [
  function () { return declaration(1, 2); },
  function () { return expression("!"); },
  function () { return arrow(1); },
  function () { return arrow(1, 2, 3, 4); },
  function () { return arrowExpr(21); },
  function () { return destructured({ a: 1 }, [3]); },
  function () { return obj.method(); },
  function () { return obj.arrowThis(); },
  function () { return obj.computed1(); },
  function () { return new Derived().hello(); },
  function () { return new Derived().priv(); },
  function () { return Derived.sderived(); },
  function () { return new Derived().fieldClosure(); },
  function () { return argumentsAcrossAwait("a", "b"); },
  function () { return sloppyThis(); },
  function () { return strictThis(); },
  function () { return strictThis.call(7); },
  function () { return paramsAreBoundBeforeBody(1); },
  function () { return varAndFunctionHoisting(); },
  function () { return closuresShareBindings(); },
  function () { return recursion(25); },
  function () { return parametersThrow(); },
  function () { var p = syncPart(); log.push("after-call"); return p; },
  function () { return awaitInFinallyOnly(); },
  function () { return evalInside(); },
  function () { return newTargetFree(); },
];
(async function () {
  for (var i = 0; i < steps.length; i++) {
    try { log.push(i + "=>" + await steps[i]()); } catch (e) { log.push(i + "!!" + e.message); }
  }
  try { new (async function () {})(); } catch (e) { log.push("construct:" + e.constructor.name); }
  log.push(Object.getPrototypeOf(declaration) === Object.getPrototypeOf(async function () {}));
  log.push(declaration.length + "," + arrow.length + "," + declaration.hasOwnProperty("prototype"));
})().then(function () {
  assert.compareArray(log, EXPECTED);
}).then($DONE, $DONE);
