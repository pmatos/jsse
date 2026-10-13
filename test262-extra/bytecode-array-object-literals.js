/*---
description: >
  Array and object literals inside compiled function bodies behave exactly as
  ArrayLiteral / ObjectLiteral evaluation specifies: element and property
  order, holes versus present `undefined`, a fresh object per evaluation,
  duplicate keys, `__proto__` setting the prototype only in its plain form,
  accessor pairs, methods with a home object, and literals larger than one
  operand chunk.
info: |
  jsse issue #959: the bytecode VM builds array and object literals natively
  (`NewArray`/`NewArrayConst`/`NewObject`) instead of handing them to the
  tree-walker. The code lives inside functions because only function bodies
  are reliably compiled.
esid: sec-array-initializer
includes: [compareArray.js, propertyHelper.js]
flags: [noStrict]
---*/

function elements(v) {
  var dense = [1, 2, 3];
  var mixed = [v, v + 1, "s", -1, -0, null, true, 7n, [v], { a: v }];
  var holes = [1, , 3, , ];
  var undef = [undefined, v === 0 ? 1 : undefined];
  return { dense: dense, mixed: mixed, holes: holes, undef: undef };
}
var r = elements(5);
assert.compareArray(r.dense, [1, 2, 3]);
assert.sameValue(r.mixed.length, 10);
assert.sameValue(r.mixed[0], 5);
assert.sameValue(r.mixed[1], 6);
assert.sameValue(r.mixed[3], -1);
assert.sameValue(1 / r.mixed[4], -Infinity, "-0 literal stays negative zero");
assert.sameValue(r.mixed[5], null);
assert.sameValue(r.mixed[7], 7n);
assert.sameValue(r.mixed[8][0], 5);
assert.sameValue(r.mixed[9].a, 5);

assert.sameValue(r.holes.length, 4, "trailing comma after an elision still counts one hole");
assert.sameValue(1 in r.holes, false, "elision creates no property");
assert.sameValue(3 in r.holes, false);
assert.sameValue(0 in r.holes, true);
assert.sameValue(Object.keys(r.holes).join(), "0,2");

assert.sameValue(0 in r.undef, true, "an explicit undefined is a present property");
assert.sameValue(1 in r.undef, true);
assert.sameValue(Object.keys(r.undef).join(), "0,1");
verifyProperty(r.undef, "0", { value: undefined, writable: true, enumerable: true, configurable: true });
verifyProperty(r.dense, "length", { value: 3, writable: true, enumerable: false, configurable: false });

function freshness() {
  var out = [];
  for (var i = 0; i < 3; i++) {
    out.push([1, 2], [i], { k: i }, function () {}, /x/);
  }
  return out;
}
var f = freshness();
for (var a = 0; a < f.length; a++) {
  for (var b = a + 1; b < f.length; b++) {
    assert.notSameValue(f[a], f[b], "literal evaluations must not share an object");
  }
}

function evaluationOrder(log) {
  return {
    first: log("first"),
    second: [log("e0"), log("e1")],
    third: log("third"),
  };
}
var order = [];
evaluationOrder(function (t) { order.push(t); return t; });
assert.compareArray(order, ["first", "e0", "e1", "third"]);

function objectShapes(v) {
  var base = { greet: function () { return "base"; } };
  var o = {
    z: 1, a: 2, 3: "three", "b c": 4, 1: "one", v, dup: 1, dup: 2,
    get acc() { return "g" + v; }, set acc(x) { this.stored = x; },
    m() { return super.greet() + v; },
    __proto__: base,
  };
  var computedProto = { "__proto__": null };
  var shorthandProto = (function (__proto__) { return { __proto__ }; })(base);
  var methodProto = { __proto__() { return "own method"; } };
  return { o: o, base: base, computedProto: computedProto, shorthandProto: shorthandProto, methodProto: methodProto };
}
var s = objectShapes("V");
assert.sameValue(Object.keys(s.o).join(), "1,3,z,a,b c,v,dup,acc,m",
  "integer keys first (ascending), then string keys in insertion order");
assert.sameValue(s.o.dup, 2, "the later duplicate wins");
assert.sameValue(s.o.acc, "gV");
s.o.acc = 9;
assert.sameValue(s.o.stored, 9);
assert.sameValue(s.o.m(), "baseV", "super resolves through the literal's home object");
assert.sameValue(Object.getPrototypeOf(s.o), s.base, "__proto__: value sets the prototype");
assert.sameValue(Object.getPrototypeOf(s.computedProto), null);
assert.sameValue(Object.prototype.hasOwnProperty.call(s.o, "__proto__"), false);
assert.sameValue(Object.prototype.hasOwnProperty.call(s.shorthandProto, "__proto__"), true,
  "shorthand __proto__ defines an own property");
assert.sameValue(Object.getPrototypeOf(s.shorthandProto), Object.prototype);
assert.sameValue(s.methodProto.__proto__(), "own method");
assert.sameValue(Object.prototype.hasOwnProperty.call(s.methodProto, "__proto__"), true);

function accessorPairs() {
  var onlyGetter = { get x() { return 1; } };
  var replaced = { get x() { return 1; }, x: 5 };
  var pair = { set x(v) {}, get x() { return 2; } };
  var dataThenAccessor = { x: 5, get x() { return 3; } };
  return [onlyGetter, replaced, pair, dataThenAccessor];
}
var ap = accessorPairs();
var d0 = Object.getOwnPropertyDescriptor(ap[0], "x");
assert.sameValue(typeof d0.get, "function");
assert.sameValue(d0.set, undefined);
assert.sameValue(d0.enumerable && d0.configurable, true);
var d1 = Object.getOwnPropertyDescriptor(ap[1], "x");
assert.sameValue(d1.value, 5, "a data property replaces an earlier accessor");
assert.sameValue(d1.get, undefined);
var d2 = Object.getOwnPropertyDescriptor(ap[2], "x");
assert.sameValue(typeof d2.get, "function");
assert.sameValue(typeof d2.set, "function");
var d3 = Object.getOwnPropertyDescriptor(ap[3], "x");
assert.sameValue(typeof d3.get, "function");
assert.sameValue(d3.value, undefined);

function largeLiterals(v) {
  var src = "(function(v) { return [";
  for (var i = 0; i < 700; i++) src += "v + " + i + ", ";
  src += "undefined, 'last']; })";
  var arr = eval(src)(v);
  var osrc = "(function(v) { return {";
  for (var j = 0; j < 600; j++) osrc += "k" + j + ": v + " + j + ", ";
  osrc += "m() { return super.x; }, get g() { return 7; }, __proto__: { x: 'px' }, k3: 'dup' }; })";
  var obj = eval(osrc)(v);
  return [arr, obj];
}
var big = largeLiterals(1);
assert.sameValue(big[0].length, 702);
assert.sameValue(big[0][0], 1);
assert.sameValue(big[0][699], 700);
assert.sameValue(big[0][700], undefined);
assert.sameValue(700 in big[0], true, "present undefined survives chunk boundaries");
assert.sameValue(big[0][701], "last");
assert.sameValue(Object.keys(big[1]).length, 602);
assert.sameValue(big[1].k599, 600);
assert.sameValue(big[1].k3, "dup");
assert.sameValue(big[1].m(), "px", "a method after the first chunk gets the home object");
assert.sameValue(big[1].g, 7);
assert.sameValue(Object.getPrototypeOf(big[1]).x, "px");
