/*---
description: Function-top-level let, const and class bindings are instantiated in TDZ on every call of one closure and of distinct closures of one literal
info: |
  10.2.11 FunctionDeclarationInstantiation

  34. For each element d of lexDeclarations, do
    a. For each element dn of the BoundNames of d, do
      i. If IsConstantDeclaration of d is true, then
        1. Perform ! lexEnv.CreateImmutableBinding(dn, true).
      ii. Else,
        1. Perform ! lexEnv.CreateMutableBinding(dn, false).

  9.1.1.1.6 GetBindingValue ( N, S ): if the binding is uninitialized, throw a ReferenceError.
  9.1.1.1.5 SetMutableBinding ( N, V, S ): an immutable binding throws a TypeError in strict code.
esid: sec-functiondeclarationinstantiation
flags: [noStrict]
---*/

function make() {
  return function (read) {
    var before;
    try {
      before = read ? tdzRead() : "skipped";
    } catch (e) {
      before = e.constructor === ReferenceError ? "ReferenceError" : "other:" + e;
    }
    function tdzRead() { return lexical; }
    let lexical = 1;
    return before + ":" + tdzRead();
  };
}

var f = make();
for (var i = 0; i < 4; i++) {
  assert.sameValue(f(true), "ReferenceError:1", "let TDZ on call " + i + " of one closure");
}
var g = make();
assert.sameValue(g(true), "ReferenceError:1", "let TDZ on first call of a distinct closure");
assert.sameValue(f(true), "ReferenceError:1", "original closure after a sibling ran");

function constTdz() {
  var first;
  try { first = c; } catch (e) { first = e.constructor; }
  const c = 7;
  return first === ReferenceError ? c : "no TDZ";
}
for (var i = 0; i < 4; i++) {
  assert.sameValue(constTdz(), 7, "const TDZ on call " + i);
}

function classTdz() {
  var first;
  try { first = new K(); } catch (e) { first = e.constructor; }
  class K { get v() { return 3; } }
  return first === ReferenceError ? new K().v : "no TDZ";
}
for (var i = 0; i < 4; i++) {
  assert.sameValue(classTdz(), 3, "class TDZ on call " + i);
}

function earlyClosure() {
  var read = function () { return late; };
  var before;
  try { before = read(); } catch (e) { before = e.constructor; }
  let late = "set";
  return [before === ReferenceError, read()];
}
for (var i = 0; i < 4; i++) {
  var r = earlyClosure();
  assert.sameValue(r[0], true, "closure created before the declaration sees TDZ, call " + i);
  assert.sameValue(r[1], "set", "same binding once initialised, call " + i);
}

function constAssign() {
  const k = 1;
  try {
    k = 2;
  } catch (e) {
    return e.constructor === TypeError ? "TypeError:" + k : "other:" + e;
  }
  return "no throw";
}
for (var i = 0; i < 4; i++) {
  assert.sameValue(constAssign(), "TypeError:1", "const reassignment on call " + i);
}

function destructuredTdz(o) {
  var inInit;
  try {
    const { a = b, b = 2 } = o;
    inInit = "no throw";
  } catch (e) {
    inInit = e.constructor === ReferenceError ? "ReferenceError" : "other:" + e;
  }
  const { x, y: [z] } = o;
  return inInit + ":" + x + z;
}
for (var i = 0; i < 4; i++) {
  assert.sameValue(destructuredTdz({ x: 1, y: [2] }), "ReferenceError:12", "destructured names, call " + i);
}

function selfTdz() {
  try {
    let v = v + 1;
  } catch (e) {
    return e.constructor === ReferenceError ? "ReferenceError" : "other:" + e;
  }
  return "no throw";
}
function selfTdzTopLevel() {
  let q = (function () { try { return w; } catch (e) { return e.constructor === ReferenceError; } })();
  let w = 1;
  return q;
}
for (var i = 0; i < 4; i++) {
  assert.sameValue(selfTdz(), "ReferenceError", "self-reference in initialiser, call " + i);
  assert.sameValue(selfTdzTopLevel(), true, "sibling TDZ in initialiser closure, call " + i);
}

function bindingOrder() {
  let a = 1;
  const b = 2;
  class C {}
  let { d, e } = { d: 4, e: 5 };
  return [a, b, typeof C, d, e].join();
}
for (var i = 0; i < 3; i++) {
  assert.sameValue(bindingOrder(), "1,2,function,4,5", "mixed lexical declarations, call " + i);
}
