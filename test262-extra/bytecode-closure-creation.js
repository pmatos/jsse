/*---
description: >
  Function and arrow literals inside compiled function bodies create closures
  exactly as InstantiateFunctionObject / InstantiateArrowFunctionExpression
  specify: NamedEvaluation of anonymous definitions, the `this`, `arguments`
  and `new.target` they capture, strictness, per-iteration and block
  environments, and the kind of function object (generator, async, method).
info: |
  jsse issue #959: `MakeClosure` creates closures from a per-chunk template
  instead of evaluating the literal on the tree-walker.
esid: sec-runtime-semantics-namedevaluation
includes: [compareArray.js, propertyHelper.js]
flags: [noStrict]
---*/

function names() {
  var a = function () {};
  let b = () => {};
  const c = async function () {};
  var d; d = function () {};
  var e; e = (0, function () {});
  var o = { m: function () {}, n: () => {}, 5: function () {}, "s p": function () {} };
  o.p = function () {};
  var named = function inner() { return typeof inner; };
  var g = function () {}; g += "";
  var h; h ||= function () {};
  var i = class {};
  return { a: a, b: b, c: c, d: d, e: e, o: o, named: named, h: h, i: i, g: g };
}
var n = names();
assert.sameValue(n.a.name, "a");
assert.sameValue(n.b.name, "b");
assert.sameValue(n.c.name, "c");
assert.sameValue(n.d.name, "d");
assert.sameValue(n.e.name, "", "a parenthesised comma expression is not an anonymous function definition");
assert.sameValue(n.o.m.name, "m");
assert.sameValue(n.o.n.name, "n");
assert.sameValue(n.o[5].name, "5");
assert.sameValue(n.o["s p"].name, "s p");
assert.sameValue(n.o.p.name, "", "a member assignment does not name the function");
assert.sameValue(n.named.name, "inner");
assert.sameValue(n.named(), "function");
assert.sameValue(n.h.name, "h", "logical assignment names its function");
assert.sameValue(n.i.name, "i");
assert.sameValue(typeof n.g, "string");
verifyProperty(n.a, "name", { value: "a", writable: false, enumerable: false, configurable: true });
verifyProperty(n.a, "length", { value: 0, writable: false, enumerable: false, configurable: true });

function kinds(arg) {
  var gen = function* () { yield 1; yield arg; };
  var asyncFn = async function () {};
  var asyncArrow = async (x) => x;
  var sloppy = function () { return this; };
  var strict = function () { "use strict"; return this; };
  var method = { m() {} }.m;
  var getter = Object.getOwnPropertyDescriptor({ get x() { return 1; } }, "x").get;
  var plain = function (p, q = 1, ...r) {};
  return { gen: gen, asyncFn: asyncFn, asyncArrow: asyncArrow, sloppy: sloppy, strict: strict,
           method: method, getter: getter, plain: plain };
}
var k = kinds("arg");
var it = k.gen();
assert.sameValue(it.next().value, 1);
assert.sameValue(it.next().value, "arg");
assert.sameValue(Object.getPrototypeOf(k.asyncFn), Object.getPrototypeOf(async function () {}));
assert.sameValue(k.asyncArrow.hasOwnProperty("prototype"), false);
assert.sameValue((0, k.sloppy)(), this, "sloppy functions see the global object");
assert.sameValue((0, k.strict)(), undefined);
assert.sameValue(k.method.hasOwnProperty("prototype"), false, "methods are not constructors");
assert.throws(TypeError, function () { new k.method(); });
assert.sameValue(k.getter.name, "get x");
assert.sameValue(k.plain.length, 1);
assert.sameValue(typeof k.sloppy.prototype, "object");
assert.sameValue(k.sloppy.prototype.constructor, k.sloppy);

function captures() {
  var viaThis = () => this;
  var viaArguments = () => arguments.length;
  var viaNewTarget = () => new.target;
  var own = function () { return arguments.length; };
  return { viaThis: viaThis, viaArguments: viaArguments, viaNewTarget: viaNewTarget, own: own };
}
var thisObj = { tag: "t" };
var c1 = captures.call(thisObj, 1, 2, 3);
assert.sameValue(c1.viaThis(), thisObj);
assert.sameValue(c1.viaArguments(), 3);
assert.sameValue(c1.viaNewTarget(), undefined);
assert.sameValue(c1.own(1), 1);
var c2 = new captures();
assert.sameValue(c2.viaNewTarget(), captures);

function loopsAndBlocks() {
  var fs = [];
  for (let i = 0; i < 3; i++) {
    let j = i * 10;
    fs.push(function () { return i + j; });
    fs.push(() => i - j);
  }
  {
    const k = "block";
    fs.push(() => k);
  }
  var vars = [];
  for (var v = 0; v < 3; v++) vars.push(function () { return v; });
  return [fs.map(function (f) { return f(); }), vars.map(function (f) { return f(); })];
}
var lb = loopsAndBlocks();
assert.compareArray(lb[0], [0, 0, 11, -9, 22, -18, "block"]);
assert.compareArray(lb[1], [3, 3, 3]);

function freshClosures() {
  var out = [];
  for (var i = 0; i < 2; i++) out.push(function () {});
  return out;
}
var fc = freshClosures();
assert.notSameValue(fc[0], fc[1], "each evaluation creates a new function object");
assert.notSameValue(fc[0].prototype, fc[1].prototype);

function immediatelyInvoked(o) {
  return [
    (function (a) { return a + 1; })(2),
    (() => 7)(),
    (function () { return function () { return 9; }; })()(),
    (0, o.m)(),
    (o.m)(),
    (o?.m)(),
    (function () { return this; })(),
    (function () { "use strict"; return this; })(),
  ];
}
var ii = immediatelyInvoked({ m: function () { return this; } });
assert.sameValue(ii[0], 3);
assert.sameValue(ii[1], 7);
assert.sameValue(ii[2], 9);
assert.sameValue(ii[3], this, "(0, o.m)() calls with an undefined this (sloppy: the global object)");
assert.sameValue(typeof ii[4], "object");
assert.notSameValue(ii[4], this, "(o.m)() keeps the reference's base as this");
assert.sameValue(ii[5], ii[4], "(o?.m)() keeps the optional chain's base as this");
assert.sameValue(ii[6], this);
assert.sameValue(ii[7], undefined);
