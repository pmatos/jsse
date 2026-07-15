/*---
description: >
  A strict-mode constructor whose body ends in a tail-position call returning a
  non-object must still yield the constructed object (or, for derived
  constructors, the super-initialized this), not the callee's value.
info: |
  10.2.2 [[Construct]] ( argumentsList, newTarget )
    ...
    12. If result.[[Type]] is return, then
      a. If result.[[Value]] is an Object, return result.[[Value]].
      b. If kind is base, return thisArgument.
    ...
    15. Return ? envRec.GetThisBinding().

  A strict-mode `return <call>` is a proper tail call. The [[Construct]]
  return-value substitution ("if the result is not an Object, return this")
  must still be applied to the resolved tail-call value; the optimization must
  not let the tail-callee's completion escape [[Construct]] verbatim.
esid: sec-ecmascript-function-objects-construct-argumentslist-newtarget
---*/

// --- Base constructor via `new`: tail call returns undefined ---
function mutate(x) { x.z = 1; } // implicitly returns undefined
function Base() {
  "use strict";
  this.a = 1;
  return mutate(this); // return <call -> undefined> in tail position
}
var base = new Base();
assert.sameValue(typeof base, "object", "new Base(): result is the constructed object");
assert.sameValue(base.a, 1, "new Base(): constructor body ran on this");
assert.sameValue(base.z, 1, "new Base(): tail-callee mutated this");

// --- Base constructor via Reflect.construct: tail call returns undefined ---
var reflected = Reflect.construct(Base, []);
assert.sameValue(typeof reflected, "object", "Reflect.construct(Base): constructed object");
assert.sameValue(reflected.a, 1, "Reflect.construct(Base): body ran");

// --- Derived constructor via `new`: tail call after super() returns undefined ---
class Sup {}
class Der extends Sup {
  constructor() {
    super();
    this.a = 1;
    return (function () { /* undefined */ })(); // tail call, non-object
  }
}
var der = new Der();
assert.sameValue(typeof der, "object", "new Der(): result is the super-initialized this");
assert.sameValue(der.a, 1, "new Der(): body ran after super()");
assert.sameValue(der instanceof Der, true, "new Der(): correct prototype chain");

// --- Derived constructor via Reflect.construct ---
var derReflected = Reflect.construct(Der, []);
assert.sameValue(typeof derReflected, "object", "Reflect.construct(Der): constructed object");
assert.sameValue(derReflected.a, 1, "Reflect.construct(Der): body ran");

// --- Guard: an OBJECT returned from a tail call still replaces this ---
function makeOther() { return { tag: 7 }; }
function ReturnsObject() {
  "use strict";
  this.a = 1;
  return makeOther(); // tail call returning an object -> replaces this
}
var obj = new ReturnsObject();
assert.sameValue(obj.tag, 7, "object return from tail call replaces this");
assert.sameValue(obj.a, undefined, "the constructed this is discarded when an object is returned");

// --- Guard: a derived constructor returning a primitive is still a TypeError ---
class DerBad extends Sup {
  constructor() {
    super();
    return 42; // not a call, not an object -> TypeError
  }
}
assert.throws(TypeError, function () { new DerBad(); },
  "derived constructor returning a primitive throws TypeError");

// --- Guard: a derived constructor returning a primitive via a TAIL CALL is
// still a TypeError (exercises the tail-call path into the same check) ---
function returnsPrimitive() { return 42; }
class DerBadTail extends Sup {
  constructor() {
    super();
    return returnsPrimitive(); // tail call returning a primitive
  }
}
assert.throws(TypeError, function () { new DerBadTail(); },
  "derived constructor returning a primitive via a tail call throws TypeError");

// --- The bignumber.js parseNumeric shape: mutate `this`, return undefined ---
function parseNumeric(x, str) { x.value = str; } // no return
function Numeric(str) {
  "use strict";
  return parseNumeric(this, str);
}
var n = new Numeric("Infinity");
assert.sameValue(typeof n, "object", "parseNumeric shape: constructed object");
assert.sameValue(n.value, "Infinity", "parseNumeric shape: this was mutated");
