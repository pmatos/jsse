// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-runtime-semantics-iteratorbindinginitialization
description: >
  A generator resumed by a for-of loop, spread, array destructuring or a
  collection constructor runs its body as a [[Call]] with no NewTarget, even
  when the loop runs inside a constructor or a native constructor is
  consuming it, and an elision in an array pattern only reads `done` of a
  result object a generator forwards from `yield*`.
includes: [compareArray.js]
features: [generators, destructuring-binding]
---*/

function* newTargetTypes() {
  yield typeof new.target;
  yield (() => typeof new.target)();
}

var viaSet = Array.from(new Set(newTargetTypes()));
assert.compareArray(viaSet, ["undefined"], "new Set(generator)");

var viaMap = new Map((function* () {
  yield [typeof new.target, 1];
})());
assert.sameValue(viaMap.get("undefined"), 1, "new Map(generator)");

function SpreadInConstructor(iterable) {
  this.values = [...iterable];
}
assert.compareArray(
  new SpreadInConstructor(newTargetTypes()).values,
  ["undefined", "undefined"],
  "spread inside a [[Construct]]"
);

function ForOfInConstructor(iterable) {
  this.values = [];
  for (var v of iterable) this.values.push(v);
}
assert.compareArray(
  new ForOfInConstructor(newTargetTypes()).values,
  ["undefined", "undefined"],
  "for-of inside a [[Construct]]"
);

function DestructureInConstructor(iterable) {
  var [first] = iterable;
  this.first = first;
}
assert.sameValue(
  new DestructureInConstructor(newTargetTypes()).first,
  "undefined",
  "array destructuring inside a [[Construct]]"
);

class ReflectBase {
  constructor(iterable) {
    this.values = Array.from(iterable);
  }
}
assert.compareArray(
  Reflect.construct(ReflectBase, [newTargetTypes()]).values,
  ["undefined", "undefined"],
  "Array.from inside a derived [[Construct]]"
);

var log = [];
var inner = {
  [Symbol.iterator]() {
    return this;
  },
  next() {
    return {
      get done() {
        log.push("done");
        return false;
      },
      get value() {
        log.push("value");
        return 1;
      }
    };
  }
};

function* forwarding() {
  yield* inner;
}

var [,] = forwarding();
assert.compareArray(
  log,
  ["done", "done"],
  "an elision reads `done` of a forwarded result object but not `value`"
);

log = [];
var [, second] = forwarding();
assert.compareArray(
  log,
  ["done", "done", "done", "done", "value"],
  "an elision followed by a binding reads `value` once, for the binding"
);
assert.sameValue(second, 1);
