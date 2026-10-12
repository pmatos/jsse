/*---
description: >
  The reference for `x = rhs` is resolved before the right-hand side runs and
  re-validated at PutValue: a RHS that deletes the property, makes it
  non-writable or declares a shadowing lexical binding is observed.
info: |
  Assignment Operators runtime semantics (sec-assignment-operators-runtime-semantics-evaluation):
  lref is evaluated first, then rval, then PutValue(lref, rval); Object
  Environment Record SetMutableBinding re-checks HasProperty
  (sec-object-environment-records-setmutablebinding-n-v-s).
esid: sec-assignment-operators-runtime-semantics-evaluation
flags: [noStrict]
---*/

globalThis.r1 = 0;
function w1() { r1 = (delete globalThis.r1, 1); }
for (var i = 0; i < 5; i++) { globalThis.r1 = 0; w1(); }
assert.sameValue(globalThis.r1, 1, "sloppy: property recreated by PutValue");

globalThis.r2 = 0;
function w2() { "use strict"; r2 = (delete globalThis.r2, 1); }
assert.throws(ReferenceError, w2, "strict: deleted before PutValue");
assert.sameValue(Object.prototype.hasOwnProperty.call(globalThis, "r2"), false);

globalThis.r3 = 0;
function w3() { r3 = (Object.defineProperty(globalThis, "r3", { writable: false }), 1); }
w3();
assert.sameValue(globalThis.r3, 0, "non-writable by the time of PutValue");

globalThis.r4 = 0;
function w4() { "use strict"; r4 += (Object.defineProperty(globalThis, "r4", { writable: false }), 1); }
assert.throws(TypeError, w4);
assert.sameValue(globalThis.r4, 0);

globalThis.r5 = 0;
function w5() { r5 = ($262.evalScript("let r5 = 'lex';"), 9); }
w5();
assert.sameValue(globalThis.r5, 0, "global property untouched");
assert.sameValue(r5, 9, "PutValue hit the lexical binding created during the RHS");
