/*---
description: >
  SetterThatIgnoresPrototypeProperties honors a module namespace exotic
  object's [[Set]] result when the receiver has an own property.
info: |
  SetterThatIgnoresPrototypeProperties ( thisValue, home, p, v )

  3. Let desc be ? thisValue.[[GetOwnProperty]](p).
  4. If desc is undefined, then
    a. Perform ? CreateDataPropertyOrThrow(thisValue, p, v).
  5. Else,
    a. Perform ? Set(thisValue, p, v, true).

  Module Namespace Exotic Objects [[Set]] ( P, V, Receiver )

  1. Return false.
esid: sec-SetterThatIgnoresPrototypeProperties
features: [error-stack-accessor, iterator-helpers, Symbol.toStringTag]
flags: [module]
---*/

import * as ns from "./SetterThatIgnoresPrototypeProperties-module-namespace-set_FIXTURE.mjs";

var errorStackSetter = Object.getOwnPropertyDescriptor(Error.prototype, "stack").set;
var iteratorConstructorSetter = Object.getOwnPropertyDescriptor(
  Iterator.prototype,
  "constructor"
).set;
var iteratorToStringTagSetter = Object.getOwnPropertyDescriptor(
  Iterator.prototype,
  Symbol.toStringTag
).set;

assert.throws(TypeError, function () {
  errorStackSetter.call(ns, "updated stack");
}, "Error.prototype.stack");
assert.throws(TypeError, function () {
  iteratorConstructorSetter.call(ns, "updated constructor");
}, "Iterator.prototype.constructor");
assert.throws(TypeError, function () {
  iteratorToStringTagSetter.call(ns, "updated tag");
}, "Iterator.prototype[Symbol.toStringTag]");

assert.sameValue(ns.stack, "original stack", "the exported stack binding was changed");
assert.sameValue(
  ns.constructor,
  "original constructor",
  "the exported constructor binding was changed"
);
assert.sameValue(ns[Symbol.toStringTag], "Module", "the namespace toStringTag was changed");
