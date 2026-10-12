// Own properties of function objects: presence, creation order and attributes.
// Function creation builds `length`, `name` and `prototype` (and, for classes,
// the class `prototype`) as ordinary own data properties; any change to how
// function objects store or materialise those properties must keep the keys,
// their relative order and their descriptors observable-identical.
// Spec: sec-ordinaryfunctioncreate (SetFunctionLength creates `length` first),
//       sec-setfunctionname (`name` next), sec-makeconstructor (`prototype`
//       {[[Writable]]: true, [[Enumerable]]: false, [[Configurable]]: false},
//       `constructor` {w:true, e:false, c:true}), sec-ordinaryownpropertykeys,
//       sec-createbuiltinfunction, sec-bound-function-exotic-objects,
//       sec-generatorfunction-instances, sec-async-function-instances,
//       sec-runtime-semantics-classdefinitionevaluation.

function Test262Error(message) {
  this.message = message || "";
}
Test262Error.prototype.toString = function () {
  return "Test262Error: " + this.message;
};

function assert(cond, label) {
  if (!cond) throw new Test262Error(label);
}

function assertDesc(obj, key, expected, label) {
  var d = Object.getOwnPropertyDescriptor(obj, key);
  assert(d !== undefined, label + ": missing own '" + String(key) + "'");
  assert("value" in d, label + ": '" + String(key) + "' is not a data property");
  assert(d.writable === expected.w, label + ": '" + String(key) + "' writable " + d.writable);
  assert(d.enumerable === expected.e, label + ": '" + String(key) + "' enumerable " + d.enumerable);
  assert(d.configurable === expected.c, label + ": '" + String(key) + "' configurable " + d.configurable);
}

function indexOfKey(keys, k) {
  return keys.indexOf(k);
}

function assertBefore(keys, a, b, label) {
  var ia = indexOfKey(keys, a);
  var ib = indexOfKey(keys, b);
  assert(ia >= 0 && ib >= 0, label + ": expected '" + a + "' and '" + b + "' in " + JSON.stringify(keys));
  assert(ia < ib, label + ": '" + a + "' must precede '" + b + "' in " + JSON.stringify(keys));
}

function assertNoRestrictedOwn(f, label) {
  var keys = Object.getOwnPropertyNames(f);
  assert(keys.indexOf("caller") < 0, label + ": unexpected own 'caller'");
  assert(keys.indexOf("arguments") < 0, label + ": unexpected own 'arguments'");
}

function assertRestrictedIfOwn(f, label) {
  ["caller", "arguments"].forEach(function (k) {
    var d = Object.getOwnPropertyDescriptor(f, k);
    if (d !== undefined) {
      assert(d.enumerable === false, label + ": own '" + k + "' must be non-enumerable");
      assert(d.configurable === true, label + ": own '" + k + "' must be configurable");
    }
  });
}

var LEN = { w: false, e: false, c: true };
var NAME = { w: false, e: false, c: true };
var PROTO = { w: true, e: false, c: false };
var CTOR = { w: true, e: false, c: true };

// Sloppy plain function.
function plain(a, b) {}
var keys = Object.getOwnPropertyNames(plain);
assert(keys[0] === "length" && keys[1] === "name", "plain: length, name first: " + JSON.stringify(keys));
assertBefore(keys, "name", "prototype", "plain");
assertDesc(plain, "length", LEN, "plain");
assertDesc(plain, "name", NAME, "plain");
assertDesc(plain, "prototype", PROTO, "plain");
assert(plain.length === 2 && plain.name === "plain", "plain: length/name values");
assertRestrictedIfOwn(plain, "plain");
assert(plain.prototype.constructor === plain, "plain: prototype.constructor");
assertDesc(plain.prototype, "constructor", CTOR, "plain.prototype");
assert(Object.getPrototypeOf(plain.prototype) === Object.prototype, "plain: prototype [[Prototype]]");
assert(Object.getOwnPropertyNames(plain.prototype).length === 1, "plain.prototype owns only constructor");

// Strict function: no caller/arguments.
var strict = function () { "use strict"; };
keys = Object.getOwnPropertyNames(strict);
assert(JSON.stringify(keys) === JSON.stringify(["length", "name", "prototype"]),
  "strict: " + JSON.stringify(keys));
assertNoRestrictedOwn(strict, "strict");

// Arrow, method, getter, setter, async function: no prototype.
var arrow = (a) => a;
assert(JSON.stringify(Object.getOwnPropertyNames(arrow)) === JSON.stringify(["length", "name"]),
  "arrow: " + JSON.stringify(Object.getOwnPropertyNames(arrow)));
assertDesc(arrow, "length", LEN, "arrow");
assertDesc(arrow, "name", NAME, "arrow");
assert(arrow.name === "arrow", "arrow name");

var o = { m() {}, get g() { return 1; }, set g(v) {}, async am() {} };
var method = o.m;
assert(JSON.stringify(Object.getOwnPropertyNames(method)) === JSON.stringify(["length", "name"]),
  "method: " + JSON.stringify(Object.getOwnPropertyNames(method)));
assertNoRestrictedOwn(method, "method");
var getter = Object.getOwnPropertyDescriptor(o, "g").get;
var setter = Object.getOwnPropertyDescriptor(o, "g").set;
assert(JSON.stringify(Object.getOwnPropertyNames(getter)) === JSON.stringify(["length", "name"]),
  "getter: " + JSON.stringify(Object.getOwnPropertyNames(getter)));
assert(getter.name === "get g" && setter.name === "set g", "accessor names");
assert(setter.length === 1, "setter length");
var asyncMethod = o.am;
assert(JSON.stringify(Object.getOwnPropertyNames(asyncMethod)) === JSON.stringify(["length", "name"]),
  "async method: " + JSON.stringify(Object.getOwnPropertyNames(asyncMethod)));
var asyncFn = async function af() {};
assert(JSON.stringify(Object.getOwnPropertyNames(asyncFn)) === JSON.stringify(["length", "name"]),
  "async function: " + JSON.stringify(Object.getOwnPropertyNames(asyncFn)));
assert(!asyncFn.hasOwnProperty("prototype"), "async function has no own prototype");

// Generators and async generators own `prototype` without `constructor`.
function* gen() {}
keys = Object.getOwnPropertyNames(gen);
assert(JSON.stringify(keys) === JSON.stringify(["length", "name", "prototype"]),
  "generator: " + JSON.stringify(keys));
assertDesc(gen, "prototype", { w: true, e: false, c: false }, "generator");
assert(Object.getOwnPropertyNames(gen.prototype).length === 0, "generator.prototype has no own keys");
assert(Object.getPrototypeOf(gen.prototype) === Object.getPrototypeOf(gen).prototype,
  "generator.prototype [[Prototype]] is %GeneratorPrototype%");
assertNoRestrictedOwn(gen, "generator");

async function* agen() {}
keys = Object.getOwnPropertyNames(agen);
assert(JSON.stringify(keys) === JSON.stringify(["length", "name", "prototype"]),
  "async generator: " + JSON.stringify(keys));
assert(Object.getOwnPropertyNames(agen.prototype).length === 0, "async generator.prototype empty");
assert(Object.getPrototypeOf(agen.prototype) === Object.getPrototypeOf(agen).prototype,
  "async generator.prototype [[Prototype]]");

var genMethod = { *gm() {} }.gm;
assert(JSON.stringify(Object.getOwnPropertyNames(genMethod)) === JSON.stringify(["length", "name", "prototype"]),
  "generator method: " + JSON.stringify(Object.getOwnPropertyNames(genMethod)));

// Classes.
class C { constructor(a, b, c) {} static m() {} }
keys = Object.getOwnPropertyNames(C);
assertBefore(keys, "length", "name", "class");
assertBefore(keys, "name", "prototype", "class");
assertBefore(keys, "prototype", "m", "class");
assertDesc(C, "length", LEN, "class");
assertDesc(C, "name", NAME, "class");
assertDesc(C, "prototype", { w: false, e: false, c: false }, "class");
assertDesc(C, "m", { w: true, e: false, c: true }, "class");
assert(C.length === 3 && C.name === "C", "class length/name");
assert(C.prototype.constructor === C, "class prototype.constructor");
assertDesc(C.prototype, "constructor", CTOR, "class.prototype");
assertNoRestrictedOwn(C, "class");
assertDesc(C.m, "length", LEN, "static method");
assert(!C.m.hasOwnProperty("prototype"), "static method has no prototype");

class D extends C {}
keys = Object.getOwnPropertyNames(D);
assert(JSON.stringify(keys) === JSON.stringify(["length", "name", "prototype"]),
  "derived class: " + JSON.stringify(keys));
assert(Object.getPrototypeOf(D) === C, "derived class [[Prototype]]");
assert(D.prototype.constructor === D, "derived prototype.constructor");

// Static members named like the built-in slots overwrite them in place.
class S1 { static name() { return "n"; } }
keys = Object.getOwnPropertyNames(S1);
assert(keys.indexOf("name") >= 0 && keys.indexOf("length") >= 0 && keys.indexOf("prototype") >= 0,
  "static name: " + JSON.stringify(keys));
assertBefore(keys, "length", "name", "static name");
assert(typeof S1.name === "function", "static name method wins");
class S2 { static length = 7; }
assert(S2.length === 7, "static length field wins");
assertBefore(Object.getOwnPropertyNames(S2), "length", "name", "static length");

// Bound functions: length, name, no prototype.
var bound = plain.bind(null, 1);
keys = Object.getOwnPropertyNames(bound);
assert(JSON.stringify(keys) === JSON.stringify(["length", "name"]), "bound: " + JSON.stringify(keys));
assertDesc(bound, "length", LEN, "bound");
assertDesc(bound, "name", NAME, "bound");
assert(bound.name === "bound plain" && bound.length === 1, "bound values");
assert(!bound.hasOwnProperty("prototype"), "bound has no own prototype");

// Built-in functions: length then name; constructors own `prototype`.
keys = Object.getOwnPropertyNames(Math.max);
assert(keys[0] === "length" && keys[1] === "name", "builtin: " + JSON.stringify(keys));
assertDesc(Math.max, "length", LEN, "Math.max");
assertDesc(Math.max, "name", NAME, "Math.max");
assert(!Math.max.hasOwnProperty("prototype"), "Math.max has no prototype");
assertBefore(Object.getOwnPropertyNames(Map), "length", "name", "Map");
assertBefore(Object.getOwnPropertyNames(Map), "name", "prototype", "Map");
assertDesc(Map, "prototype", { w: false, e: false, c: false }, "Map");

// Promise resolving functions.
var resolveFn;
new Promise(function (res) { resolveFn = res; });
keys = Object.getOwnPropertyNames(resolveFn);
assert(JSON.stringify(keys) === JSON.stringify(["length", "name"]), "resolve fn: " + JSON.stringify(keys));
assert(resolveFn.length === 1 && resolveFn.name === "", "resolve fn values");

// Functions created through the Function constructor.
var dyn = Function("a", "b", "return a + b");
keys = Object.getOwnPropertyNames(dyn);
assertBefore(keys, "length", "name", "dynamic");
assertBefore(keys, "name", "prototype", "dynamic");
assert(dyn.name === "anonymous" && dyn.length === 2, "dynamic values");
assert(dyn.prototype.constructor === dyn, "dynamic prototype.constructor");

// %ThrowTypeError%.
var thrower = Object.getOwnPropertyDescriptor(Function.prototype, "caller").get;
assert(thrower === Object.getOwnPropertyDescriptor(Function.prototype, "arguments").get,
  "%ThrowTypeError% is shared");
assert(Object.isFrozen(thrower), "%ThrowTypeError% is frozen");
assert(JSON.stringify(Object.getOwnPropertyNames(thrower)) === JSON.stringify(["length", "name"]),
  "ThrowTypeError keys: " + JSON.stringify(Object.getOwnPropertyNames(thrower)));
assertDesc(thrower, "length", { w: false, e: false, c: false }, "ThrowTypeError");
assertDesc(thrower, "name", { w: false, e: false, c: false }, "ThrowTypeError");

// Cross-realm function objects have the same shape.
if (typeof $262 !== "undefined" && typeof $262.createRealm === "function") {
  var other = $262.createRealm().global;
  var ofn = other.eval("(function foo(a) {})");
  keys = Object.getOwnPropertyNames(ofn);
  assertBefore(keys, "length", "name", "cross-realm");
  assertBefore(keys, "name", "prototype", "cross-realm");
  assert(Object.getPrototypeOf(ofn.prototype) === other.Object.prototype,
    "cross-realm prototype [[Prototype]] is the other realm's Object.prototype");
  assert(ofn.prototype.constructor === ofn, "cross-realm prototype.constructor");
}
