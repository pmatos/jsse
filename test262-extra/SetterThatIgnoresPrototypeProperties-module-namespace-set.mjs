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

import * as ns from "./SetterThatIgnoresPrototypeProperties-module-namespace-set-dep.mjs";

var errorStackSetter = Object.getOwnPropertyDescriptor(Error.prototype, "stack").set;
var iteratorConstructorSetter = Object.getOwnPropertyDescriptor(
  Iterator.prototype,
  "constructor"
).set;
var iteratorToStringTagSetter = Object.getOwnPropertyDescriptor(
  Iterator.prototype,
  Symbol.toStringTag
).set;

function assertTypeError(setter, value, label) {
  var caught;
  try {
    setter.call(ns, value);
  } catch (error) {
    caught = error;
  }
  if (!(caught instanceof TypeError)) {
    throw new Test262Error(label + ": expected TypeError, got " + caught);
  }
}

assertTypeError(errorStackSetter, "updated stack", "Error.prototype.stack");
assertTypeError(iteratorConstructorSetter, "updated constructor", "Iterator.prototype.constructor");
assertTypeError(
  iteratorToStringTagSetter,
  "updated tag",
  "Iterator.prototype[Symbol.toStringTag]"
);

if (ns.stack !== "original stack") {
  throw new Test262Error("the exported stack binding was changed");
}
if (ns.constructor !== "original constructor") {
  throw new Test262Error("the exported constructor binding was changed");
}
if (ns[Symbol.toStringTag] !== "Module") {
  throw new Test262Error("the namespace toStringTag was changed");
}
