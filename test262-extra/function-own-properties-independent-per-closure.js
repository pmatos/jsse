// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-function-instances
description: >
  Closures of one function literal start with the same own keys and attributes
  (length, name, sloppy caller/arguments, prototype) but each is independent:
  redefining, deleting or freezing a property of one closure is invisible to
  the others, and properties added later keep creation order.
info: |
  10.2.4 FunctionAllocate / 10.2.8 SetFunctionName / 10.2.9 SetFunctionLength
  20.2.4.1 length, 20.2.4.2 name
  B.3.4 Changes to FunctionDeclarationInstantiation (sloppy caller/arguments)
includes: [propertyHelper.js, compareArray.js]
flags: [noStrict]
features: [Reflect, arrow-function, class]
---*/

// Whether a sloppy function also has own "caller"/"arguments" is up to the
// implementation (they are not part of the required own keys), so the key
// checks below look through them.
function keysOf(f) {
  return Reflect.ownKeys(f).filter(function (k) { return k !== "caller" && k !== "arguments"; });
}

function makeSloppy() { return function (a, b) { return a + b; }; }
function makeNamed() { return function inner(a) { return a; }; }
function makeArrow() { return (a, b, c) => a; }
function makeStrict() { "use strict"; return function (a) { return a; }; }

var s1 = makeSloppy(), s2 = makeSloppy();
assert.compareArray(keysOf(s1), ["length", "name", "prototype"], "sloppy keys");
assert.compareArray(keysOf(s2), keysOf(s1), "sloppy keys of a second closure");
verifyProperty(s1, "length", { value: 2, writable: false, enumerable: false, configurable: true }, { restore: true });
verifyProperty(s2, "name", { value: "", writable: false, enumerable: false, configurable: true }, { restore: true });
assert.sameValue(Object.prototype.hasOwnProperty.call(s2, "name"), true, "name restored on the second closure");

// Writes and deletions on the first closure never reach the second.
Object.defineProperty(s1, "name", { value: "renamed" });
Object.defineProperty(s1, "length", { value: 9 });
assert.sameValue(Object.getOwnPropertyDescriptor(s1, "length").configurable, true, "redefined length stays configurable");
assert.sameValue(s1.name, "renamed", "redefined name");
assert.sameValue(s1.length, 9, "redefined length");
assert.sameValue(s2.name, "", "second closure keeps its name");
assert.sameValue(s2.length, 2, "second closure keeps its length");

var s3 = makeSloppy();
delete s3.length;
assert.sameValue(Object.prototype.hasOwnProperty.call(s3, "length"), false, "length deleted");
assert.compareArray(keysOf(s3), ["name", "prototype"], "keys after delete");
assert.sameValue(makeSloppy().length, 2, "later closures still have length");


// A property added to a closure comes after the leading ones and stays private.
var s5 = makeSloppy(), s6 = makeSloppy();
s5.extra = 1;
Object.defineProperty(s5, "late", { value: 2, enumerable: true, configurable: true });
assert.compareArray(keysOf(s5), ["length", "name", "prototype", "extra", "late"], "added keys last");
assert.compareArray(keysOf(s6), ["length", "name", "prototype"], "other closure has no extra keys");
assert.sameValue(s6.extra, undefined, "extra is not shared");

// Many properties push the object past the linear property list.
var many = makeSloppy();
for (var i = 0; i < 20; i++) { many["k" + i] = i; }
assert.sameValue(keysOf(many).length, 23, "key count after many additions");
assert.sameValue(many.k19, 19, "last added value");
assert.sameValue(many.name, "", "leading entries survive the spill");
assert.sameValue(makeSloppy().k0, undefined, "other closure unaffected by the spill");

// Freezing makes the shared-looking entries non-writable on that closure only.
var s7 = makeSloppy(), s8 = makeSloppy();
Object.freeze(s7);
assert.sameValue(Object.isFrozen(s7), true, "frozen");
assert.sameValue(Object.isFrozen(s8), false, "sibling not frozen");
verifyProperty(s8, "prototype", { writable: true, enumerable: false, configurable: false });
verifyProperty(s7, "prototype", { writable: false, enumerable: false, configurable: false });
s8.prototype = {};
assert.sameValue(Object.getPrototypeOf(new s8()), s8.prototype, "sibling prototype assignable");

// Each closure has its own prototype object whose constructor points back.
var p1 = makeSloppy(), p2 = makeSloppy();
assert.notSameValue(p1.prototype, p2.prototype, "distinct prototype objects");
assert.sameValue(p1.prototype.constructor, p1, "constructor of the first");
assert.sameValue(p2.prototype.constructor, p2, "constructor of the second");

// Named function expressions: the name is the literal's, in a scope of its own.
var n1 = makeNamed(), n2 = makeNamed();
assert.sameValue(n1.name, "inner", "named function expression name");
assert.sameValue(n2.name, "inner", "second named closure");
Object.defineProperty(n1, "name", { value: "other" });
assert.sameValue(n2.name, "inner", "named closure keeps its name");
assert.sameValue(typeof inner, "undefined", "function expression name does not leak");
var self = function selfRef() { return selfRef; };
assert.sameValue(self(), self, "name binding refers to the closure");
var selfMaker = function () { return function selfRef2() { selfRef2 = 1; return typeof selfRef2; }; };
assert.sameValue(selfMaker()(), "function", "sloppy assignment to the name binding is ignored");

// Arrows and strict functions have no caller/arguments and no arrow prototype.
var a1 = makeArrow(), a2 = makeArrow();
assert.compareArray(keysOf(a1), ["length", "name"], "arrow keys");
assert.sameValue(a1.length, 3, "arrow length");
a1.extra = 1;
assert.compareArray(keysOf(a2), ["length", "name"], "arrow sibling keys");
assert.compareArray(keysOf(makeStrict()), ["length", "name", "prototype"], "strict keys");

// Inferred names are per evaluation, not per literal.
var names = [];
function collect(o) { names.push(o.name); }
var objs = [{ a: function () {} }, { b: function () {} }];
for (var o of objs) { for (var key in o) { collect(o[key]); } }
var computed = [];
for (var k of ["x", "y"]) { var o2 = { [k]: function () {} }; computed.push(o2[k].name); }
assert.compareArray(computed, ["x", "y"], "computed key names differ per evaluation of one literal");
assert.compareArray(names, ["a", "b"], "inferred names of different literals");
var holder = {};
for (var j = 0; j < 2; j++) { holder["h" + j] = function () {}; }
assert.sameValue(holder.h0.name, "", "assignment to a member expression does not name the function");

// Classes without a constructor share a synthesized one; their names differ.
class A {}
class B {}
class C extends A {}
class D extends A {}
assert.compareArray([A.name, B.name, C.name, D.name], ["A", "B", "C", "D"], "default constructors keep their own names");
assert.compareArray(keysOf(A), ["length", "name", "prototype"], "class keys");
verifyProperty(B, "name", { value: "B", writable: false, enumerable: false, configurable: true });
var E = class { static name() { return 1; } };
assert.sameValue(typeof E.name, "function", "static name method replaces the name property");
var F = class { static name() { return 1; } };
assert.sameValue(typeof F.name, "function", "second class with a static name method");

// Function declarations hoisted in each call.
function hoisting() { function decl(x) { return x; } return decl; }
var d1 = hoisting(), d2 = hoisting();
assert.sameValue(d1.name, "decl", "hoisted declaration name");
Object.defineProperty(d1, "name", { value: "z" });
assert.sameValue(d2.name, "decl", "sibling hoisted declaration keeps its name");
assert.sameValue(hoisting().length, 1, "hoisted declaration length");

// A hoisted declaration replaces a binding of the same name already in scope.
function overParameter(a) { function a() { return 1; } return typeof a; }
assert.sameValue(overParameter(5), "function", "declaration replaces a parameter");
function overVar() { var g = 5; function g() {} return typeof g; }
assert.sameValue(overVar(), "number", "var initializer runs after hoisting");
function overVarNoInit() { var g; function g() {} return typeof g; }
assert.sameValue(overVarNoInit(), "function", "bare var keeps the hoisted function");
function overArguments() { function arguments() {} return typeof arguments; }
assert.sameValue(overArguments(), "function", "declaration named arguments");
function twice() { function h() { return 1; } function h() { return 2; } return h(); }
assert.sameValue(twice(), 2, "the last of two same-name declarations wins");
function evalDeclared() { eval("function e() { return 3; }"); return e(); }
assert.sameValue(evalDeclared(), 3, "function declared by direct eval");
