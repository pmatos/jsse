/*---
description: >
  typeof, instanceof, the in operator and BigInt literals behave identically
  when the enclosing function is compiled to bytecode.
info: |
  jsse issue #902: each of these used to send the entire function to the
  tree-walker. They now compile to dedicated opcodes.

  The typeof Operator (13.5.3.1), InstanceofOperator (13.10.2),
  OrdinaryHasInstance (7.3.21), HasProperty (7.3.11), NumericValue of
  numeric literals (12.9.3).
esid: sec-typeof-operator-runtime-semantics-evaluation
flags: [noStrict]
---*/

function typeofResults(o) {
  var u;
  return [
    typeof u, typeof 1, typeof "s", typeof true, typeof o, typeof null,
    typeof function () {}, typeof Symbol(), typeof 1n,
    typeof undeclared_binding_902, typeof o.missing
  ].join();
}
assert.sameValue(
  typeofResults({}),
  "undefined,number,string,boolean,object,object,function,symbol,bigint,undefined,undefined"
);

function typeofHtmlDDA() {
  return typeof $262.IsHTMLDDA;
}
assert.sameValue(typeofHtmlDDA(), "undefined");

function typeofInTdzThrows() {
  var caught;
  try {
    typeof later;
  } catch (e) {
    caught = e.name;
  }
  let later = 1;
  return caught;
}
assert.sameValue(typeofInTdzThrows(), "ReferenceError");

function typeofUnderWith() {
  var scope = { present: 1 };
  with (scope) {
    return typeof present + "," + typeof absent_902;
  }
}
assert.sameValue(typeofUnderWith(), "number,undefined");

function instanceofForms() {
  function C() {}
  var c = new C();
  var hook = { [Symbol.hasInstance](v) { return v === 1; } };
  var bound = C.bind(null);
  return [
    c instanceof C, c instanceof Object, 1 instanceof hook, 2 instanceof hook,
    c instanceof bound, {} instanceof C
  ].join();
}
assert.sameValue(instanceofForms(), "true,true,true,false,true,false");

function instanceofRejectsNonCallable() {
  var r = [];
  try { ({}) instanceof 1; } catch (e) { r.push(e.constructor === TypeError); }
  try { ({}) instanceof {}; } catch (e) { r.push(e.constructor === TypeError); }
  return r.join();
}
assert.sameValue(instanceofRejectsNonCallable(), "true,true");

function inOperatorForms() {
  var seen = [];
  var p = new Proxy({}, {
    has: function (t, k) { seen.push(String(k)); return k === "q"; }
  });
  var sym = Symbol("s");
  var o = {};
  o[sym] = 1;
  return [
    "q" in p, "z" in p, 0 in [5], 1 in [5], sym in o, "length" in [],
    seen.join("")
  ].join();
}
assert.sameValue(inOperatorForms(), "true,false,true,false,true,true,qz");

function inRejectsPrimitives() {
  try { "x" in 1; } catch (e) { return e.constructor === TypeError; }
  return false;
}
assert.sameValue(inRejectsPrimitives(), true);

function bigintLiterals() {
  var a = 10n ** 5n;
  return [a, 0x1Fn, 0o7n, 0b101n, 1n + 2n, typeof 0n, 0n === 0n, 2n > 1n].join();
}
assert.sameValue(bigintLiterals(), "100000,31,7,5,3,bigint,true,true");

function bigintMixedThrows() {
  try { return 1n + 1; } catch (e) { return e.constructor === TypeError; }
}
assert.sameValue(bigintMixedThrows(), true);

function regexpLiteralIsFreshEachEvaluation() {
  function make() { return /a/g; }
  return make() === make();
}
assert.sameValue(regexpLiteralIsFreshEachEvaluation(), false);
