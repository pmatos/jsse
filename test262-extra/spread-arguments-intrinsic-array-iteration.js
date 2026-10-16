// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-argument-lists-runtime-semantics-argumentlistevaluation
description: >
  Spreading a plain Array into call and `new` arguments copies its elements
  without observable iterator traffic, but every way user code can observe the
  iteration protocol still takes effect: a replaced Array.prototype[@@iterator],
  a replaced %ArrayIteratorPrototype%.next, an own @@iterator, a changed
  prototype, and holes answered by an indexed accessor on the prototype chain.
features: [Symbol.iterator, Reflect]
---*/

function collect() {
  return Array.prototype.slice.call(arguments);
}

function sameList(actual, expected, message) {
  assert.sameValue(actual.length, expected.length, message + " length");
  for (var i = 0; i < expected.length; i++) {
    assert.sameValue(actual[i], expected[i], message + " [" + i + "]");
  }
}

sameList(collect(...[]), [], "empty array");
sameList(collect(...[1, 2, 3]), [1, 2, 3], "three elements");
sameList(collect(0, ...[1, 2], 3, ...[4]), [0, 1, 2, 3, 4], "mixed with plain arguments");

var withUndefined = [1, undefined, 3];
sameList(collect(...withUndefined), [1, undefined, 3], "present undefined element");

var holes = [1, , 3];
var spreadHoles = collect(...holes);
assert.sameValue(spreadHoles.length, 3, "hole counts as an argument");
assert.sameValue(1 in spreadHoles, true, "hole is passed as a present undefined");
assert.sameValue(spreadHoles[1], undefined, "hole reads undefined");

function Ctor() {
  this.args = collect.apply(null, arguments);
}
sameList(new Ctor(...[7, 8]).args, [7, 8], "new with spread");

var original = [1, 2, 3];
var seen;
function mutate(a) {
  original.push(99);
  original[1] = "changed";
  seen = arguments.length;
}
mutate(...original);
assert.sameValue(seen, 3, "arguments were fixed before the call");

var iteratorCalls = 0;
var savedValues = Array.prototype[Symbol.iterator];
Array.prototype[Symbol.iterator] = function () {
  iteratorCalls++;
  return savedValues.call(this);
};
sameList(collect(...[1, 2]), [1, 2], "patched @@iterator result");
assert.sameValue(iteratorCalls, 1, "patched Array.prototype[@@iterator] is called");
Array.prototype[Symbol.iterator] = function () {
  return savedValues.call([10, 20]);
};
sameList(collect(...[1, 2, 3]), [10, 20], "patched @@iterator supplies the values");
Array.prototype[Symbol.iterator] = savedValues;
sameList(collect(...[1, 2]), [1, 2], "restored @@iterator");

var ArrayIteratorPrototype = Object.getPrototypeOf([][Symbol.iterator]());
var savedNext = ArrayIteratorPrototype.next;
var nextCalls = 0;
ArrayIteratorPrototype.next = function () {
  nextCalls++;
  return savedNext.call(this);
};
sameList(collect(...[5, 6]), [5, 6], "patched next result");
assert.sameValue(nextCalls, 3, "patched %ArrayIteratorPrototype%.next runs per step and for done");
ArrayIteratorPrototype.next = savedNext;
nextCalls = 0;
sameList(collect(...[5, 6]), [5, 6], "restored next");
assert.sameValue(nextCalls, 0, "restored next is not the patched one");

var own = [1, 2, 3];
own[Symbol.iterator] = function* () {
  yield "own";
};
sameList(collect(...own), ["own"], "own @@iterator");

var reparented = [1, 2, 3];
Object.setPrototypeOf(reparented, {
  [Symbol.iterator]: function* () {
    yield "reparented";
  }
});
sameList(collect(...reparented), ["reparented"], "array with another prototype");

class Sub extends Array {
  *[Symbol.iterator]() {
    yield "sub";
  }
}
sameList(collect(...Sub.from([1, 2])), ["sub"], "Array subclass iterator");

var getterCalls = [];
Object.defineProperty(Array.prototype, 1, {
  get: function () {
    getterCalls.push("get1");
    return "inherited";
  },
  configurable: true
});
try {
  var viaHole = collect(...[0, , 2]);
  sameList(viaHole, [0, "inherited", 2], "hole answered by prototype getter");
  assert.sameValue(getterCalls.length, 1, "getter ran once");
} finally {
  delete Array.prototype[1];
}

var proxied = new Proxy([1, 2], {});
sameList(collect(...proxied), [1, 2], "proxy of an array");

var args = (function () {
  return arguments;
})(1, 2);
sameList(collect(...args), [1, 2], "arguments object");

var lengthHidden = [1, 2, 3];
Object.defineProperty(lengthHidden, "length", { writable: false });
sameList(collect(...lengthHidden), [1, 2, 3], "non-writable length");

var frozen = Object.freeze([4, 5]);
sameList(collect(...frozen), [4, 5], "frozen array");

var withAccessor = [1, 2, 3];
var accessorReads = 0;
Object.defineProperty(withAccessor, 1, {
  get: function () {
    accessorReads++;
    return "got";
  },
  enumerable: true,
  configurable: true
});
sameList(collect(...withAccessor), [1, "got", 3], "accessor element");
assert.sameValue(accessorReads, 1, "accessor element read once");

sameList([...[1, 2, 3]], [1, 2, 3], "array literal spread");
sameList([0, ...[1, , 3]], [0, 1, undefined, 3], "array literal spread with a hole");
assert.sameValue(new AggregateError([1, 2, 3]).errors.length, 3, "AggregateError errors list");
sameList(new AggregateError([4, 5]).errors, [4, 5], "AggregateError errors values");

var literalCalls = 0;
Array.prototype[Symbol.iterator] = function () {
  literalCalls++;
  return savedValues.call(this);
};
sameList([...[1, 2]], [1, 2], "array literal spread through a patched @@iterator");
assert.sameValue(literalCalls, 1, "array literal spread observes the patched @@iterator");
sameList(new AggregateError([7, 8]).errors, [7, 8], "AggregateError through a patched @@iterator");
assert.sameValue(literalCalls, 2, "AggregateError observes the patched @@iterator");
Array.prototype[Symbol.iterator] = savedValues;

ArrayIteratorPrototype.next = function () {
  nextCalls++;
  return savedNext.call(this);
};
nextCalls = 0;
sameList([...[1]], [1], "array literal spread through a patched next");
assert.sameValue(nextCalls, 2, "array literal spread observes the patched next");
ArrayIteratorPrototype.next = savedNext;

var objectGetterCalls = 0;
Object.defineProperty(Object.prototype, 1, {
  get: function () {
    objectGetterCalls++;
    return "from-object-prototype";
  },
  configurable: true
});
try {
  sameList(collect(...[0, , 2]), [0, "from-object-prototype", 2], "hole answered by Object.prototype");
  sameList([...[0, , 2]], [0, "from-object-prototype", 2], "literal hole answered by Object.prototype");
  assert.sameValue(objectGetterCalls, 2, "Object.prototype getter ran once per spread");
} finally {
  delete Object.prototype[1];
}
