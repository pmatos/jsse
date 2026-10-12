/*---
description: >
  The default derived constructor forwards its arguments to the parent
  constructor without iterating them and then initializes its own class's
  fields, whatever path constructs it: Reflect.construct (with or without a
  separate newTarget), a subclass's super(), a bound function or a Proxy
  without a construct trap.
esid: sec-runtime-semantics-classdefinitionevaluation
info: |
  ClassDefinitionEvaluation (15.7.14)
    14. If constructor is empty, then
      a. Let defaultConstructor be a new Abstract Closure with no parameters
         that captures nothing and performs the following steps when called:
         ...
         ii. If NewTarget is undefined, throw a TypeError exception.
         iii. Let F be the active function object.
         iv. If F.[[ConstructorKind]] is derived, then
           1. NOTE: This branch behaves similarly to constructor(...args) {
              super(...args); }. The most notable distinction is that while
              the aforementioned ECMAScript source text observably calls the
              %Symbol.iterator% method on %Array.prototype%, this function
              does not.
           2. Let func be ! F.[[GetPrototypeOf]]().
           3. If IsConstructor(func) is false, throw a TypeError exception.
           4. Let result be ? Construct(func, args, NewTarget).
         ...
         vi. Perform ? InitializeInstanceElements(result, F).
includes: [compareArray.js]
features: [class-fields-public, Reflect.construct, Proxy, Symbol.iterator]
---*/

var iteratorCalls = 0;
var originalIterator = Array.prototype[Symbol.iterator];
Array.prototype[Symbol.iterator] = function () {
  iteratorCalls++;
  return originalIterator.call(this);
};

class Parent {
  constructor(x, y) {
    this.sum = x + y;
  }
}
class Child extends Parent {
  field = this.sum * 10;
}

var viaReflect = Reflect.construct(Child, [1, 2]);
assert.sameValue(viaReflect.sum, 3, "Reflect.construct forwards the arguments");
assert.sameValue(viaReflect.field, 30, "Reflect.construct initializes the fields");

function OtherTarget() {}
OtherTarget.prototype = { marker: true };
var viaNewTarget = Reflect.construct(Child, [2, 2], OtherTarget);
assert.sameValue(Object.getPrototypeOf(viaNewTarget), OtherTarget.prototype, "newTarget picks the prototype");
assert.sameValue(viaNewTarget.field, 40, "a separate newTarget still initializes the class's fields");

class GrandChild extends Child {
  constructor() {
    super(3, 4);
  }
}
var viaSuper = new GrandChild();
assert.sameValue(viaSuper.sum, 7, "super() forwards the arguments");
assert.sameValue(viaSuper.field, 70, "super() into a default constructor initializes its fields");

var Bound = Child.bind(null, 5);
var viaBound = new Bound(5);
assert.sameValue(viaBound.sum, 10, "a bound function forwards the arguments");
assert.sameValue(viaBound.field, 100, "a bound function initializes the fields");

var viaProxy = new (new Proxy(Child, {}))(1, 1);
assert.sameValue(viaProxy.sum, 2, "a Proxy without a construct trap forwards the arguments");
assert.sameValue(viaProxy.field, 20, "a Proxy without a construct trap initializes the fields");

Array.prototype[Symbol.iterator] = originalIterator;
assert.sameValue(iteratorCalls, 0, "no construct path iterates the arguments");

class Swapped extends Parent {
  own = "own";
}
Object.setPrototypeOf(Swapped, function Replacement() {
  this.replaced = true;
});
var swapped = new Swapped();
assert.compareArray(Object.keys(swapped), ["replaced", "own"], "the parent is read from F.[[GetPrototypeOf]]() at construct time");

Object.setPrototypeOf(Swapped, Math.max);
assert.throws(TypeError, function () {
  Reflect.construct(Swapped, []);
}, "a non-constructor parent throws a TypeError");

var method = {
  m() {
    this.methodRan = true;
  }
}.m;
Object.setPrototypeOf(Swapped, method);
assert.throws(TypeError, function () {
  new Swapped();
}, "a method parent is not a constructor");

var trapCalls = 0;
Object.setPrototypeOf(Swapped, new Proxy(() => {}, {
  construct: function () {
    trapCalls++;
    return {};
  }
}));
assert.throws(TypeError, function () {
  new Swapped();
}, "a Proxy whose target is not a constructor is not a constructor");
assert.sameValue(trapCalls, 0, "the construct trap of a non-constructor Proxy is never called");
