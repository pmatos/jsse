// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-generatorresume
description: >
  A generator resumed by for-of, spread, destructuring or a collection
  constructor runs with its own execution context: `new.target` inside the
  body is undefined even when the consumer runs inside a constructor, and an
  array-pattern elision over a generator that delegates with `yield*` reads
  only `done` of the forwarded result object, never `value`.
info: |
  27.5.3.3 GeneratorResume: the body runs in the generator's own execution
  context, so NewTarget there is undefined.
  13.15.5.5 IteratorDestructuringAssignmentEvaluation: an Elision performs
  IteratorStep only, which reads `done` and not `value`.
includes: [compareArray.js]
features: [Symbol.iterator, generators, new.target]
---*/

function* targets() {
  yield typeof new.target;
  yield (() => typeof new.target)();
}

function Spread(it) {
  this.items = [...it];
}
assert.compareArray(new Spread(targets()).items, ["undefined", "undefined"], "spread in constructor");

function ForOf(it) {
  this.items = [];
  for (var v of it) this.items.push(v);
}
assert.compareArray(new ForOf(targets()).items, ["undefined", "undefined"], "for-of in constructor");

function Destructure(it) {
  var [a, b] = it;
  this.items = [a, b];
}
assert.compareArray(new Destructure(targets()).items, ["undefined", "undefined"], "destructuring in constructor");

function Collection(it) {
  this.items = Array.from(new Set(it));
}
assert.compareArray(new Collection(targets()).items, ["undefined"], "Set constructor in constructor");

function Own() {
  return new.target;
}
function Outer(it) {
  var seen = [...it];
  this.after = Own() === undefined;
  this.items = seen;
}
var outer = new Outer(targets());
assert.sameValue(outer.after, true, "new.target of the constructor is intact after the loop");

// Elision over a delegating generator: only done is read.
var log = [];
var inner = {
  [Symbol.iterator]: function () {
    return this;
  },
  next: function () {
    return {
      get done() {
        log.push("done");
        return false;
      },
      get value() {
        log.push("value");
        return 1;
      },
    };
  },
  return: function () {
    log.push("return");
    return {};
  },
};
function* delegating() {
  yield* inner;
}
var [,] = delegating();
assert.compareArray(log.filter(function (e) { return e !== "return"; }).sort(), ["done", "done"], "elision reads done only");
